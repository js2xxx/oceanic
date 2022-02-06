use alloc::{string::String, sync::Arc, vec::Vec};
use core::{hint, slice, time::Duration};

use paging::LAddr;
use spin::Mutex;
use sv_call::*;

use super::{Blocked, RunningState, Signal, Space, Tid};
use crate::{
    cpu::time::Instant,
    sched::{imp::MIN_TIME_GRAN, Arsc, PREEMPT, SCHED},
    syscall::{In, InOut, Out, UserPtr},
};

#[derive(Debug)]
struct SuspendToken {
    slot: Arsc<Mutex<Option<super::Blocked>>>,
    tid: Tid,
}

impl SuspendToken {
    #[inline]
    pub fn signal(&self) -> Signal {
        Signal::Suspend(Arsc::clone(&self.slot))
    }
}

impl Drop for SuspendToken {
    fn drop(&mut self) {
        match super::PREEMPT.scope(|| self.slot.lock().take()) {
            Some(task) => SCHED.unblock(task, true),
            None => self.tid.with_signal(|sig| {
                if matches!(sig, Some(sig) if sig == &mut self.signal()) {
                    *sig = None;
                }
            }),
        }
    }
}

#[syscall]
fn task_exit(retval: usize) -> Result {
    SCHED.exit_current(retval);
    #[allow(unreachable_code)]
    Err(Error::EKILLED)
}

#[syscall]
fn task_sleep(ms: u32) -> Result {
    if ms == 0 {
        let _ = SCHED.with_current(|cur| {
            cur.running_state = RunningState::NEED_RESCHED;
            Ok(())
        });
        SCHED.tick(Instant::now());

        Ok(())
    } else {
        SCHED
            .block_current((), None, Duration::from_millis(u64::from(ms)), "task_sleep")
            .map(|_| ())
    }
}

#[syscall]
fn space_new() -> Result<Handle> {
    SCHED.with_current(|cur| {
        let space = Space::new(cur.tid().ty());
        cur.space().handles().insert(space)
    })
}

#[syscall]
fn task_exec(ci: UserPtr<In, task::ExecInfo>) -> Result<Handle> {
    let ci = unsafe { ci.read()? };
    ci.init_chan.check_null()?;

    let name = {
        let ptr = UserPtr::<In, _>::new(ci.name as *mut u8);
        if !ptr.as_ptr().is_null() {
            let name_len = ci.name_len;
            let mut buf = Vec::<u8>::with_capacity(name_len);
            unsafe {
                ptr.read_slice(buf.as_mut_ptr(), name_len)?;
                buf.set_len(name_len);
            }
            Some(String::from_utf8(buf).map_err(|_| Error::EINVAL)?)
        } else {
            None
        }
    };

    let (init_chan, space) = SCHED.with_current(|cur| {
        let init_chan = cur
            .space()
            .handles()
            .remove::<crate::sched::ipc::Channel>(ci.init_chan)?;
        if ci.space == Handle::NULL {
            Ok((init_chan, Arsc::clone(cur.space())))
        } else {
            cur.space()
                .handles()
                .remove::<Arsc<Space>>(ci.space)?
                .downcast_ref::<Arsc<Space>>()
                .map(|space| (init_chan, Arsc::clone(space)))
        }
    })?;

    let init_chan = PREEMPT.scope(|| unsafe { space.handles().insert_ref(init_chan) })?;

    UserPtr::<In, _>::new(ci.entry).check()?;
    UserPtr::<In, _>::new(ci.stack).check()?;

    let starter = super::Starter {
        entry: LAddr::new(ci.entry),
        stack: LAddr::new(ci.stack),
        arg: ci.arg,
    };
    let (task, hdl) = super::exec(name, space, init_chan, &starter)?;

    SCHED.unblock(task, true);

    Ok(hdl)
}

#[syscall]
fn task_new(
    name: UserPtr<In, u8>,
    name_len: usize,
    space: Handle,
    st: UserPtr<Out, Handle>,
) -> Result<Handle> {
    let name = {
        if !name.as_ptr().is_null() {
            let mut buf = Vec::<u8>::with_capacity(name_len);
            unsafe {
                name.read_slice(buf.as_mut_ptr(), name_len)?;
                buf.set_len(name_len);
            }
            Some(String::from_utf8(buf).map_err(|_| Error::EINVAL)?)
        } else {
            None
        }
    };

    let new_space = if space == Handle::NULL {
        SCHED.with_current(|cur| Ok(Arsc::clone(cur.space())))?
    } else {
        SCHED.with_current(|cur| {
            cur.space()
                .handles()
                .remove::<Arsc<Space>>(space)?
                .downcast_ref::<Arsc<Space>>()
                .map(|space| Arsc::clone(space))
        })?
    };
    let mut sus_slot = Arsc::try_new_uninit()?;

    let (task, hdl) = super::create(name, Arsc::clone(&new_space))?;

    let task = super::Ready::block(
        super::IntoReady::into_ready(task, unsafe { crate::cpu::id() }, MIN_TIME_GRAN),
        "task_ctl_suspend",
    );

    let tid = task.tid().clone();
    let st_data = unsafe {
        Arsc::get_mut_unchecked(&mut sus_slot).write(Mutex::new(Some(task)));
        SuspendToken {
            slot: Arsc::assume_init(sus_slot),
            tid,
        }
    };
    SCHED.with_current(|cur| {
        let st_h = cur.space().handles().insert(st_data)?;
        unsafe { st.write(st_h) }
    })?;

    Ok(hdl)
}

#[syscall]
fn task_join(hdl: Handle) -> Result<usize> {
    hdl.check_null()?;

    let obj = SCHED.with_current(|cur| cur.space().handles().remove::<Tid>(hdl))?;
    let tid = obj.downcast_ref::<Tid>()?;

    PREEMPT.scope(|| tid.ret_cell().lock().ok_or(Error::ENOENT))
}

#[syscall]
fn task_ctl(hdl: Handle, op: u32, data: UserPtr<InOut, Handle>) -> Result {
    hdl.check_null()?;

    let cur = SCHED.with_current(|cur| Ok(Arsc::clone(cur.space())))?;

    match op {
        task::TASK_CTL_KILL => {
            let child = cur.child(hdl)?;
            child.with_signal(|sig| *sig = Some(Signal::Kill));

            Ok(())
        }
        task::TASK_CTL_SUSPEND => {
            data.out().check()?;

            let child = cur.child(hdl)?;

            let st = SuspendToken {
                slot: Arsc::try_new(Mutex::new(None))?,
                tid: child,
            };

            st.tid.with_signal(|sig| {
                if sig == &Some(Signal::Kill) {
                    Err(Error::EPERM)
                } else {
                    *sig = Some(st.signal());
                    Ok(())
                }
            })?;

            let out = super::PREEMPT.scope(|| cur.handles().insert(st))?;
            unsafe { data.out().write(out)? };

            Ok(())
        }
        _ => Err(Error::EINVAL),
    }
}

fn read_regs(task: &Blocked, addr: usize, data: UserPtr<Out, u8>, len: usize) -> Result<()> {
    match addr {
        task::TASK_DBGADDR_GPR => {
            if len < task::ctx::GPR_SIZE {
                Err(Error::EBUFFER)
            } else {
                unsafe { data.cast().write(task.kstack().task_frame().debug_get()) }
            }
        }
        task::TASK_DBGADDR_FPU => {
            let size = archop::fpu::frame_size();
            if len < size {
                Err(Error::EBUFFER)
            } else {
                unsafe { data.write_slice(&task.ext_frame()[..size]) }
            }
        }
        _ => Err(Error::EINVAL),
    }
}

fn write_regs(task: &mut Blocked, addr: usize, data: UserPtr<In, u8>, len: usize) -> Result<()> {
    match addr {
        task::TASK_DBGADDR_GPR => {
            if len < sv_call::task::ctx::GPR_SIZE {
                Err(Error::EBUFFER)
            } else {
                let gpr = unsafe { data.cast().read()? };
                unsafe { task.kstack_mut().task_frame_mut().debug_set(&gpr) }
            }
        }
        task::TASK_DBGADDR_FPU => {
            let size = archop::fpu::frame_size();
            if len < size {
                Err(Error::EBUFFER)
            } else {
                let ptr = task.ext_frame_mut().as_mut_ptr();
                unsafe { data.read_slice(ptr, size) }
            }
        }
        _ => Err(Error::EINVAL),
    }
}

fn create_excep_chan(task: &Blocked) -> Result<crate::sched::ipc::Channel> {
    let slot = task.tid().excep_chan();
    let chan = match slot.lock() {
        mut g if g.is_none() => {
            let (usr, krl) = crate::sched::ipc::Channel::new();
            *g = Some(krl);
            usr
        }
        _ => return Err(Error::EEXIST),
    };
    Ok(chan)
}

#[syscall]
fn task_debug(hdl: Handle, op: u32, addr: usize, data: UserPtr<InOut, u8>, len: usize) -> Result {
    hdl.check_null()?;
    data.check_slice(len)?;

    let slot = SCHED.with_current(|cur| {
        cur.space()
            .handles()
            .get::<SuspendToken>(hdl)
            .map(|st| Arsc::clone(&st.slot))
    })?;

    let mut task = loop {
        match super::PREEMPT.scope(|| slot.lock().take()) {
            Some(task) => break task,
            _ => hint::spin_loop(),
        }
    };

    let ret = match op {
        task::TASK_DBG_READ_REG => read_regs(&task, addr, data.out(), len),
        task::TASK_DBG_WRITE_REG => write_regs(&mut task, addr, data.r#in(), len),
        task::TASK_DBG_READ_MEM => unsafe {
            crate::mem::space::with(task.space().mem(), |_| {
                let slice = slice::from_raw_parts(addr as *mut u8, len);
                data.out().write_slice(slice)
            })
        },
        task::TASK_DBG_WRITE_MEM => unsafe {
            crate::mem::space::with(task.space().mem(), |_| {
                data.r#in().read_slice(addr as *mut u8, len)
            })
        },
        task::TASK_DBG_EXCEP_HDL => {
            if len < core::mem::size_of::<Handle>() {
                Err(Error::EBUFFER)
            } else {
                let hdl = SCHED.with_current(|cur| {
                    create_excep_chan(&task).and_then(|chan| unsafe {
                        let event = Arc::downgrade(chan.event()) as _;
                        cur.space()
                            .handles()
                            .insert_unchecked(chan, true, false, event)
                    })
                })?;

                unsafe { data.out().cast::<Handle>().write(hdl) }
            }
        }
        _ => Err(Error::EINVAL),
    };

    PREEMPT.scope(|| *slot.lock() = Some(task));
    ret
}
