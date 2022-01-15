use alloc::{string::String, sync::Arc, vec::Vec};
use core::{hint, slice, time::Duration};

use paging::LAddr;
use solvent::*;
use spin::Mutex;

use super::{Blocked, RunningState, Signal, Tid, DEFAULT_STACK_SIZE};
use crate::{
    cpu::time::Instant,
    mem::space,
    sched::{sched::MIN_TIME_GRAN, PREEMPT, SCHED},
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
fn task_sleep(ms: u32) {
    if ms == 0 {
        SCHED.with_current(|cur| cur.running_state = RunningState::NEED_RESCHED);
        SCHED.tick(Instant::now());
    } else {
        SCHED.block_current((), None, Duration::from_millis(u64::from(ms)), "task_sleep");
    }
    Ok(())
}

#[syscall]
fn task_fn(
    ci: UserPtr<In, task::CreateInfo>,
    cf: task::CreateFlags,
    extra: UserPtr<Out, Handle>,
) -> Handle {
    let ci = unsafe { ci.read()? };
    if cf.contains(task::CreateFlags::SUSPEND_ON_START) {
        extra.check()?;
    }
    ci.init_chan.check_null()?;

    let name = {
        let ptr = UserPtr::<In, _>::new(ci.name);
        if !ptr.as_ptr().is_null() {
            let mut buf = Vec::<u8>::with_capacity(ci.name_len);
            unsafe { ptr.read_slice(buf.as_mut_ptr(), buf.len()) }?;
            Some(String::from_utf8(buf).map_err(|_| Error(EINVAL))?)
        } else {
            None
        }
    };

    let stack_size = if ci.stack_size == 0 {
        DEFAULT_STACK_SIZE
    } else {
        ci.stack_size
    };

    let init_chan = SCHED
        .with_current(|cur| {
            cur.tid()
                .handles()
                .remove::<crate::sched::ipc::Channel>(ci.init_chan)
        })
        .ok_or(Error(ESRCH))?
        .ok_or(Error(EINVAL))?;

    UserPtr::<In, _>::new(ci.func).check()?;

    let (task, hdl) = super::create_fn(
        name,
        None,
        None,
        None,
        LAddr::new(ci.func),
        init_chan,
        ci.arg as u64,
        stack_size,
    )
    .map_err(Into::into)?;

    if cf.contains(task::CreateFlags::SUSPEND_ON_START) {
        let task = super::Ready::block(
            super::IntoReady::into_ready(task, unsafe { crate::cpu::id() }, MIN_TIME_GRAN),
            "task_ctl_suspend",
        );
        let tid = task.tid().clone();
        let st = SuspendToken {
            slot: Arc::new(Mutex::new(Some(task))),
            tid,
        };
        let st = SCHED
            .with_current(|cur| cur.tid.handles().insert(st))
            .unwrap();
        unsafe { extra.write(st) }?;
    } else {
        SCHED.unblock(task);
    }

    Ok(hdl)
}

#[syscall]
fn task_join(hdl: Handle) -> usize {
    hdl.check_null()?;

    let child = {
        let tid = SCHED
            .with_current(|cur| cur.tid.clone())
            .ok_or(Error(ESRCH))?;

        tid.drop_child(hdl).ok_or(Error(ECHILD))?
    };

    Error::decode(child.ret_cell().take(Duration::MAX, "task_join").unwrap())
}

#[syscall]
fn task_ctl(hdl: Handle, op: u32, data: UserPtr<InOut, Handle>) {
    hdl.check_null()?;

    let cur_tid = SCHED
        .with_current(|cur| cur.tid.clone())
        .ok_or(Error(ESRCH))?;

    match op {
        task::TASK_CTL_KILL => {
            let child = cur_tid.child(hdl).ok_or(Error(ECHILD))?;
            child.with_signal(|sig| *sig = Some(Signal::Kill));

            Ok(())
        }
        task::TASK_CTL_SUSPEND => {
            data.out().check()?;

            let child = cur_tid.child(hdl).ok_or(Error(ECHILD))?;

            let st = SuspendToken {
                slot: Arc::new(Mutex::new(None)),
                tid: child.clone(),
            };

            st.tid.with_signal(|sig| {
                if sig == &Some(Signal::Kill) {
                    Err(Error(EPERM))
                } else {
                    *sig = Some(st.signal());
                    Ok(())
                }
            })?;

            let out = super::PREEMPT.scope(|| cur_tid.handles().insert(st));
            unsafe { data.out().write(out) }.unwrap();

            Ok(())
        }
        task::TASK_CTL_DETACH => {
            if cur_tid.drop_child(hdl).is_some() {
                Ok(())
            } else {
                Err(Error(ECHILD))
            }
        }
        _ => Err(Error(EINVAL)),
    }
}

fn read_regs(task: &Blocked, addr: usize, data: UserPtr<Out, u8>, len: usize) -> Result<()> {
    match addr {
        task::TASK_DBGADDR_GPR => {
            if len < task::ctx::GPR_SIZE {
                Err(Error(EBUFFER))
            } else {
                unsafe { task.kstack().task_frame().debug_get(data.cast()) }
            }
        }
        task::TASK_DBGADDR_FPU => {
            let size = archop::fpu::frame_size();
            if len < size {
                Err(Error(EBUFFER))
            } else {
                unsafe { data.write_slice(&task.ext_frame()[..size]) }
            }
        }
        _ => Err(Error(EINVAL)),
    }
}

fn write_regs(task: &mut Blocked, addr: usize, data: UserPtr<In, u8>, len: usize) -> Result<()> {
    match addr {
        task::TASK_DBGADDR_GPR => {
            if len < solvent::task::ctx::GPR_SIZE {
                Err(Error(EBUFFER))
            } else {
                let gpr = unsafe { data.cast().read()? };
                unsafe { task.kstack_mut().task_frame_mut().debug_set(&gpr) }
            }
        }
        task::TASK_DBGADDR_FPU => {
            let size = archop::fpu::frame_size();
            if len < size {
                Err(Error(EBUFFER))
            } else {
                let ptr = task.ext_frame_mut().as_mut_ptr();
                unsafe { data.read_slice(ptr, size) }
            }
        }
        _ => Err(Error(EINVAL)),
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
        _ => return Err(Error(EEXIST)),
    };
    Ok(chan)
}

#[syscall]
fn task_debug(hdl: Handle, op: u32, addr: usize, data: UserPtr<InOut, u8>, len: usize) {
    hdl.check_null()?;
    data.check_slice(len)?;

    let slot = SCHED
        .with_current(|cur| {
            cur.tid
                .handles()
                .get::<SuspendToken>(hdl)
                .map(|st| Arc::clone(&st.slot))
        })
        .ok_or(Error(ESRCH))?
        .ok_or(Error(EINVAL))?;

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
            space::with(&task.space(), |_| {
                let slice = slice::from_raw_parts(addr as *mut u8, len);
                data.out().write_slice(slice)
            })
        },
        task::TASK_DBG_WRITE_MEM => unsafe {
            space::with(&task.space(), |_| {
                data.r#in().read_slice(addr as *mut u8, len)
            })
        },
        task::TASK_DBG_EXCEP_HDL => {
            if len < core::mem::size_of::<Handle>() {
                Err(Error(EBUFFER))
            } else {
                let hdl = SCHED
                    .with_current(|cur| {
                        create_excep_chan(&task).map(|chan| cur.tid.handles().insert(chan))
                    })
                    .unwrap()?;
                unsafe { data.out().cast().write(hdl) }
            }
        }
        _ => Err(Error(EINVAL)),
    };

    PREEMPT.scope(|| *slot.lock() = Some(task));
    ret
}
