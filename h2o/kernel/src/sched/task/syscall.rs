use alloc::{string::String, sync::Arc, vec::Vec};
use core::{hint, slice, time::Duration};

use paging::LAddr;
use solvent::*;
use spin::Mutex;

use super::{Blocked, RunningState, Signal, Tid};
use crate::{
    cpu::time::Instant,
    mem::space,
    sched::{imp::MIN_TIME_GRAN, PREEMPT, SCHED},
    syscall::{In, InOut, Out, UserPtr},
};

#[derive(Debug)]
struct SuspendToken {
    slot: Arc<Mutex<Option<super::Blocked>>>,
    tid: Tid,
}

impl SuspendToken {
    #[inline]
    pub fn signal(&self) -> Signal {
        Signal::Suspend(Arc::clone(&self.slot))
    }
}

impl Drop for SuspendToken {
    fn drop(&mut self) {
        match super::PREEMPT.scope(|| self.slot.lock().take()) {
            Some(task) => SCHED.unblock(task),
            None => self.tid.with_signal(|sig| {
                if matches!(sig, Some(sig) if sig == &mut self.signal()) {
                    *sig = None;
                }
            }),
        }
    }
}

#[syscall]
fn task_exit(retval: usize) {
    SCHED.exit_current(retval);
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
            .tid()
            .handles()
            .remove::<crate::sched::ipc::Channel>(ci.init_chan)?;
        if ci.space == Handle::NULL {
            Ok((init_chan, Arc::clone(cur.space())))
        } else {
            cur.tid()
                .handles()
                .remove::<Arc<space::Space>>(ci.space)?
                .downcast_ref::<Arc<space::Space>>()
                .map(|space| (init_chan, Arc::clone(space)))
        }
    })?;

    UserPtr::<In, _>::new(ci.entry).check()?;
    UserPtr::<In, _>::new(ci.stack).check()?;

    let starter = super::Starter {
        entry: LAddr::new(ci.entry),
        stack: LAddr::new(ci.stack),
        arg: ci.arg,
    };
    let (task, hdl) = super::exec(name, space, init_chan, &starter)?;

    SCHED.unblock(task);

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
        space::with_current(Arc::clone)
    } else {
        SCHED.with_current(|cur| {
            cur.tid()
                .handles()
                .remove::<Arc<space::Space>>(space)?
                .downcast_ref::<Arc<space::Space>>()
                .map(|space| Arc::clone(space))
        })?
    };

    let (task, hdl) = super::create(name, Arc::clone(&new_space))?;

    let task = super::Ready::block(
        super::IntoReady::into_ready(task, unsafe { crate::cpu::id() }, MIN_TIME_GRAN),
        "task_ctl_suspend",
    );
    let tid = task.tid().clone();
    let st_data = SuspendToken {
        slot: Arc::new(Mutex::new(Some(task))),
        tid,
    };
    SCHED.with_current(|cur| {
        let st_h = cur.tid().handles().insert(st_data)?;
        unsafe { st.write(st_h) }
    })?;

    Ok(hdl)
}

#[syscall]
fn task_join(hdl: Handle) -> Result<usize> {
    hdl.check_null()?;

    let child = SCHED.with_current(|cur| {
        cur.tid
            .handles()
            .remove::<Tid>(hdl)
            .and_then(|w| w.downcast_ref::<Tid>().map(|w| Tid::clone(w)))
    })?;
    Ok(child.ret_cell().take(Duration::MAX, "task_join").unwrap())
}

#[syscall]
fn task_ctl(hdl: Handle, op: u32, data: UserPtr<InOut, Handle>) -> Result {
    hdl.check_null()?;

    let cur_tid = SCHED.with_current(|cur| Ok(cur.tid.clone()))?;

    match op {
        task::TASK_CTL_KILL => {
            let child = cur_tid.child(hdl)?;
            child.with_signal(|sig| *sig = Some(Signal::Kill));

            Ok(())
        }
        task::TASK_CTL_SUSPEND => {
            data.out().check()?;

            let child = cur_tid.child(hdl)?;

            let st = SuspendToken {
                slot: Arc::new(Mutex::new(None)),
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

            let out = super::PREEMPT.scope(|| cur_tid.handles().insert(st))?;
            unsafe { data.out().write(out) }.unwrap();

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
            if len < solvent::task::ctx::GPR_SIZE {
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
        cur.tid
            .handles()
            .get::<SuspendToken>(hdl)
            .map(|st| Arc::clone(&st.slot))
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
            space::with(task.space(), |_| {
                let slice = slice::from_raw_parts(addr as *mut u8, len);
                data.out().write_slice(slice)
            })
        },
        task::TASK_DBG_WRITE_MEM => unsafe {
            space::with(task.space(), |_| {
                data.r#in().read_slice(addr as *mut u8, len)
            })
        },
        task::TASK_DBG_EXCEP_HDL => {
            if len < core::mem::size_of::<Handle>() {
                Err(Error::EBUFFER)
            } else {
                let hdl = SCHED.with_current(|cur| {
                    create_excep_chan(&task).and_then(|chan| cur.tid.handles().insert(chan))
                })?;

                unsafe { data.out().cast::<Handle>().write(hdl) }
            }
        }
        _ => Err(Error::EINVAL),
    };

    PREEMPT.scope(|| *slot.lock() = Some(task));
    ret
}
