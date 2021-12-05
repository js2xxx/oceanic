use alloc::{string::ToString, sync::Arc};
use core::time::Duration;

use paging::LAddr;
use solvent::*;

use super::{RunningState, Signal};
use crate::{
    cpu::time::Instant,
    sched::{wait::WaitObject, SCHED},
    syscall::{In, UserPtr},
};

#[syscall]
pub fn task_exit(retval: usize) {
    SCHED.exit_current(retval);
}

#[syscall]
fn task_sleep(ms: u32) {
    if ms == 0 {
        SCHED.with_current(|cur| cur.running_state = RunningState::NeedResched);
        SCHED.tick(Instant::now());
    } else {
        SCHED.sleep_current(Duration::from_millis(u64::from(ms)));
    }
    Ok(())
}

#[syscall]
pub fn task_fn(
    name: UserPtr<In, u8>,
    name_len: usize,
    stack_size: usize,
    func: *mut u8,
    arg: *mut u8,
) -> u32 {
    let name = name.null_or_slice(name_len, |ptr| {
        ptr.map(|ptr| unsafe {
            let slice = ptr.as_ref();

            core::str::from_utf8(slice)
                .map_err(|_| Error(EINVAL))
                .map(ToString::to_string)
        })
        .transpose()
    })?;

    let (task, ret_wo) =
        super::create_fn(name, stack_size, LAddr::new(func), arg).map_err(Into::into)?;
    SCHED.push(task);

    Ok(ret_wo.raw())
}

#[syscall]
pub fn task_join(hdl: Handle) -> usize {
    hdl.check_null()?;

    let child = {
        let tid = SCHED
            .with_current(|cur| cur.tid.clone())
            .ok_or(Error(ESRCH))?;

        let _pree = super::PREEMPT.lock();
        tid.child(hdl).ok_or(Error(ECHILD))?
    };

    Error::decode(child.cell().take("task_join"))
}

#[syscall]
pub fn task_ctl(hdl: Handle, op: u32, data: *mut u8) {
    hdl.check_null()?;

    let tid = SCHED
        .with_current(|cur| cur.tid.clone())
        .ok_or(Error(ESRCH))?;

    match op {
        task::TASK_CTL_KILL => {
            let child = {
                let _pree = super::PREEMPT.lock();
                tid.child(hdl).ok_or(Error(ECHILD))?
            };

            let _pree = super::PREEMPT.lock();
            let mut ti = child.tid().info().write();
            ti.replace_signal(Some(Signal::Kill));

            Ok(())
        }
        task::TASK_CTL_SUSPEND => {
            let wo = {
                let data = Handle::try_from(data)?;

                let _pree = super::PREEMPT.lock();
                let info = &tid.info().read();
                match info.handles.get::<Arc<WaitObject>>(data).cloned() {
                    Some(wo) => wo,
                    None => return Err(Error(EINVAL)),
                }
            };

            let child = {
                let _pree = super::PREEMPT.lock();
                tid.child(hdl).ok_or(Error(ECHILD))?
            };

            let _pree = super::PREEMPT.lock();
            let mut ti = child.tid().info().write();
            ti.replace_signal(Some(Signal::Suspend(wo)));

            Ok(())
        }
        task::TASK_CTL_DETACH => {
            let _pree = super::PREEMPT.lock();
            let mut ti = tid.info().write();
            ti.handles.remove(hdl).ok_or(Error(ECHILD))?;

            Ok(())
        }
        _ => Err(Error(EINVAL)),
    }
}
