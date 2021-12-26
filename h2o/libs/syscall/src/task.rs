use crate::Handle;

pub const DEFAULT_STACK_SIZE: usize = 256 * 1024;

pub const PRIO_DEFAULT: u16 = 20;

pub const TASK_CTL_KILL: u32 = 1;
pub const TASK_CTL_SUSPEND: u32 = 2;
pub const TASK_CTL_DETACH: u32 = 3;

#[derive(Debug)]
#[repr(C)]
pub struct CreateInfo {
    pub name: *mut u8,
    pub name_len: usize,
    pub stack_size: usize,
    pub init_chan: Handle,
    pub func: *mut u8,
    pub arg: *mut u8,
}

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
    extern "C" fn func(_: crate::Handle, arg: u32) {
        match arg {
            0 => {
                for _ in 0..10000000 {
                    unsafe { asm!("pause") };
                }
            }
            _ => {}
        }
        exit(Ok(12345));
    }

    let creator = |arg: u32| {
        let ci = CreateInfo {
            name: core::ptr::null_mut(),
            name_len: 0,
            stack_size: crate::task::DEFAULT_STACK_SIZE,
            init_chan: Handle::NULL,
            func: func as *mut u8,
            arg: arg as *mut u8,
        };
        crate::call::task_fn(&ci)
    };
    {
        let task = creator(100).expect("Failed to create task");
        let ret = crate::call::task_join(task);
        assert_eq!(ret, Ok(12345));
    }
    {
        let task = creator(0).expect("Failed to create task");
        let mut st = Handle::NULL;

        crate::call::task_ctl(task, TASK_CTL_SUSPEND, &mut st).expect("Failed to suspend a task");

        crate::call::task_sleep(50).expect("Failed to sleep");
        crate::call::obj_drop(st).expect("Failed to resume the task");

        crate::call::task_ctl(task, TASK_CTL_KILL, core::ptr::null_mut())
            .expect("Failed to kill a task");

        let ret = crate::call::task_join(task);
        assert_eq!(ret, Err(crate::Error(crate::EKILLED)));
    }
}
