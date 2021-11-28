pub const DEFAULT_STACK_SIZE: usize = 256 * 1024;

pub const PRIO_DEFAULT: u16 = 20;

pub const TASK_CTL_KILL: u32 = 1;
pub const TASK_CTL_SUSPEND: u32 = 2;
pub const TASK_CTL_DETACH: u32 = 3;

#[cfg(feature = "call")]
pub fn exit<T>(res: crate::Result<T>) -> !
where
    T: crate::SerdeReg,
{
    let retval = crate::Error::encode(res.map(|o| o.encode()));
    let _ = crate::call::task_exit(retval);
    unreachable!();
}

#[cfg(feature = "call")]
#[cfg(debug_assertions)]
pub fn test() {
    use core::hint;

    extern "C" fn func(arg: u32) {
        if arg == 0 {
            for _ in 0..10000000 {}
        }
        exit(Ok(12345));
    }

    let creator = |arg: u32| {
        crate::call::task_fn(
            core::ptr::null_mut(),
            crate::task::DEFAULT_STACK_SIZE,
            func as *mut u8,
            arg as *mut u8,
        )
    };
    {
        let task = creator(1).expect("Failed to create task");
        let ret = crate::call::task_join(task);
        assert_eq!(ret, Ok(12345));
    }
    {
        let task = creator(0).expect("Failed to create task");
        let wo = crate::call::wo_create().expect("Failed to create wait object");

        crate::call::task_ctl(task, TASK_CTL_SUSPEND, wo as *mut u8)
            .expect("Failed to suspend a task");
        hint::spin_loop();

        let notify = || crate::call::wo_notify(wo, 0).expect("Failed to notify a wait object");
        let mut n = notify();
        while n == 0 {
            n = notify();
            hint::spin_loop();
        }
        assert_eq!(n, 1);

        crate::call::task_ctl(task, TASK_CTL_KILL, core::ptr::null_mut())
            .expect("Failed to kill a task");

        let ret = crate::call::task_join(task);
        assert_eq!(ret, Err(crate::Error(crate::EKILLED)));
    }
}
