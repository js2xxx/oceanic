use solvent::*;

#[syscall]
pub fn task_exit(retval: usize) {
    crate::sched::SCHED.exit_current(retval);
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
    crate::sched::SCHED.push(task);

    Ok(ret_wo.raw())
}

#[syscall]
pub fn task_join(hdl: u32) -> usize {
    use core::num::NonZeroU32;

    let wc_hdl = super::UserHandle::new(NonZeroU32::new(hdl).ok_or(Error(EINVAL))?);

    let child = {
        let _intr = archop::IntrState::lock();
        let tid = crate::sched::SCHED
            .with_current(|cur| cur.tid.clone())
            .ok_or(Error(ESRCH))?;

        tid.child(wc_hdl).ok_or(Error(ECHILD))?
    };

    let _intr = archop::IntrState::lock();
    solvent::Error::decode(child.cell().take("task_join"))
}

#[syscall]
pub fn task_ctl(hdl: u32, op: u32) {
    use core::num::NonZeroU32;

    match op {
        // Kill
        1 => {
            let child = {
                let _intr = archop::IntrState::lock();
                let tid = crate::sched::SCHED
                    .with_current(|cur| cur.tid.clone())
                    .ok_or(Error(ESRCH))?;

                let wc_hdl = super::UserHandle::new(NonZeroU32::new(hdl).ok_or(Error(EINVAL))?);
                tid.child(wc_hdl).ok_or(Error(ECHILD))?
            };

            let _intr = archop::IntrState::lock();
            let mut ti = child.tid().info().write();
            ti.set_signal(Some(super::Signal::Kill));

            Ok(())
        }
        // Suspend
        2 => todo!(),
        _ => Err(Error(EINVAL)),
    }
}
