use alloc::sync::Arc;

use solvent::*;

use crate::sched::{wait::WaitObject, SCHED};

#[syscall]
pub fn task_exit(retval: usize) {
    SCHED.exit_current(retval);
}

#[syscall]
fn task_sleep(ms: u32) {
    if ms > 0 {
        return Err(Error(EINVAL));
    }
    SCHED.with_current(|cur| cur.running_state = super::RunningState::NeedResched);
    SCHED.tick(crate::cpu::time::Instant::now());
    Ok(())
}

#[syscall]
pub fn task_fn(name: *mut u8, stack_size: usize, func: *mut u8, arg: *mut u8) -> u32 {
    extern "C" {
        fn strlen(s: *const u8) -> usize;
    }
    use crate::alloc::string::ToString;

    let name = if !name.is_null() {
        unsafe {
            let slice = core::slice::from_raw_parts(name, strlen(name));
            Some(
                core::str::from_utf8(slice)
                    .map_err(|_| Error(EINVAL))?
                    .to_string(),
            )
        }
    } else {
        None
    };

    let (task, ret_wo) =
        super::create_fn(name, stack_size, paging::LAddr::new(func), arg).map_err(Into::into)?;
    SCHED.push(task);

    Ok(ret_wo.raw())
}

#[syscall]
pub fn task_join(hdl: u32) -> usize {
    use core::num::NonZeroU32;

    let wc_hdl = super::UserHandle::new(NonZeroU32::new(hdl).ok_or(Error(EINVAL))?);

    let child = {
        let tid = SCHED
            .with_current(|cur| cur.tid.clone())
            .ok_or(Error(ESRCH))?;

        let _pree = super::PREEMPT.lock();
        tid.child(wc_hdl).ok_or(Error(ECHILD))?
    };

    solvent::Error::decode(child.cell().take("task_join"))
}

#[syscall]
pub fn task_ctl(hdl: u32, op: u32, data: *mut u8) {
    use core::num::NonZeroU32;

    let wc_hdl = super::UserHandle::new(NonZeroU32::new(hdl).ok_or(Error(EINVAL))?);

    let tid = SCHED
        .with_current(|cur| cur.tid.clone())
        .ok_or(Error(ESRCH))?;

    match op {
        task::TASK_CTL_KILL => {
            let child = {
                let _pree = super::PREEMPT.lock();
                tid.child(wc_hdl).ok_or(Error(ECHILD))?
            };

            let _pree = super::PREEMPT.lock();
            let mut ti = child.tid().info().write();
            ti.replace_signal(Some(super::Signal::Kill));

            Ok(())
        }
        task::TASK_CTL_SUSPEND => {
            let wo = {
                let data = u32::try_from(data as u64)
                    .ok()
                    .and_then(NonZeroU32::new)
                    .map(super::UserHandle::new)
                    .ok_or(Error(EINVAL))?;

                let _pree = super::PREEMPT.lock();
                let info = &tid.info().read();
                match info.user_handles.get::<Arc<WaitObject>>(data).cloned() {
                    Some(wo) => wo,
                    None => return Err(Error(EINVAL)),
                }
            };

            let child = {
                let _pree = super::PREEMPT.lock();
                tid.child(wc_hdl).ok_or(Error(ECHILD))?
            };

            let _pree = super::PREEMPT.lock();
            let mut ti = child.tid().info().write();
            ti.replace_signal(Some(super::Signal::Suspend(wo)));

            Ok(())
        }
        task::TASK_CTL_DETACH => {
            let _pree = super::PREEMPT.lock();
            let mut ti = tid.info().write();
            ti.user_handles.remove(wc_hdl).ok_or(Error(ECHILD))?;

            Ok(())
        }
        _ => Err(Error(EINVAL)),
    }
}
