use core::{arch::asm, mem::MaybeUninit};

use super::{
    ctx::{Gpr, GPR_SIZE},
    *,
};
use crate::call::*;

extern "C" fn func(_: crate::Handle, arg: u32) {
    match arg {
        0 => {
            let a = 19837476.238647f64;
            let b = a.recip();
            let c = b.recip();
            assert!((c - a) < 0.00001 && (a - c) < 0.00001);
            for _ in 0..10000000 {
                unsafe { asm!("pause") };
            }
        }
        1 => unsafe {
            let addr: usize = 0x2341_0000_0000_0000;
            let ptr: *mut u64 = core::mem::transmute(addr);
            *ptr = 1;
        },
        _ => {}
    }
    exit(Ok(12345i32));
}

fn join(normal: Handle, fault: Handle) {
    let ret = task_join(normal);
    assert_eq!(ret, Ok(12345));

    let ret = task_join(fault);
    assert_eq!(ret, Err(crate::Error(crate::EFAULT)));
}

fn sleep() {
    task_sleep(50).expect("Failed to sleep");
}

fn debug_mem(st: Handle) {
    let mut buf = [0u8; 15];
    task_debug(st, TASK_DBG_READ_MEM, 0x401000, buf.as_mut_ptr(), buf.len())
        .expect("Failed to read memory");
    let ret = task_debug(
        st,
        TASK_DBG_WRITE_MEM,
        0x401000,
        buf.as_mut_ptr(),
        buf.len(),
    );
    assert_eq!(ret, Err(crate::Error(crate::EPERM)));
}

fn debug_reg_gpr(st: Handle) {
    let mut buf = MaybeUninit::<Gpr>::uninit();
    task_debug(
        st,
        TASK_DBG_READ_REG,
        TASK_DBGADDR_GPR,
        buf.as_mut_ptr().cast(),
        GPR_SIZE,
    )
    .expect("Failed to read general registers");
    task_debug(
        st,
        TASK_DBG_WRITE_REG,
        TASK_DBGADDR_GPR,
        buf.as_mut_ptr().cast(),
        GPR_SIZE,
    )
    .expect("Failed to write general registers");
}

fn debug_reg_fpu(st: Handle) {
    let mut buf = [0u8; 576];
    task_debug(
        st,
        TASK_DBG_READ_REG,
        TASK_DBGADDR_FPU,
        buf.as_mut_ptr(),
        buf.len(),
    )
    .expect("Failed to read FPU registers");
    task_debug(
        st,
        TASK_DBG_WRITE_REG,
        TASK_DBGADDR_FPU,
        buf.as_mut_ptr(),
        buf.len(),
    )
    .expect("Failed to write FPU registers");
}

fn suspend(task: Handle) {
    let mut st = Handle::NULL;

    task_ctl(task, TASK_CTL_SUSPEND, &mut st).expect("Failed to suspend a task");
    sleep();

    {
        debug_mem(st);
        debug_reg_gpr(st);
        debug_reg_fpu(st);
    }

    obj_drop(st).expect("Failed to resume the task");
}

fn kill(task: Handle) {
    task_ctl(task, TASK_CTL_KILL, core::ptr::null_mut()).expect("Failed to kill a task");

    let ret = task_join(task);
    assert_eq!(ret, Err(crate::Error(crate::EKILLED)));
}

fn ctl(task: Handle) {
    suspend(task);
    sleep();
    kill(task);
}

pub fn test() {
    // Test the defence of invalid user pointer access.
    let ret = task_fn(0x100000000 as *const CreateInfo);
    assert_eq!(ret, Err(crate::Error(crate::EPERM)));

    let creator = |arg: u32| {
        let ci = CreateInfo {
            name: core::ptr::null_mut(),
            name_len: 0,
            stack_size: crate::task::DEFAULT_STACK_SIZE,
            init_chan: Handle::NULL,
            func: func as *mut u8,
            arg: arg as *mut u8,
        };
        task_fn(&ci)
    };

    join(
        creator(100).expect("Failed to create task"),
        creator(1).expect("Failed to create task"),
    );

    ctl(creator(0).expect("Failed to create task"));
}
