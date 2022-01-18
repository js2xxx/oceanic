use core::{
    arch::asm,
    mem::{size_of, MaybeUninit},
    ptr::null_mut,
};

use super::{
    ctx::{Gpr, GPR_SIZE},
    excep::Exception,
    *,
};
use crate::{call::*, task::excep::ExceptionResult};

const PF_ADDR: usize = 0x1598_0000_0000;

extern "C" fn func(_: crate::Handle, arg: u32) {
    log::trace!("arg = {}", arg);
    match arg {
        0 => {
            let a = 19837476.238647f64;
            let b = a.recip();
            let c = b.recip();
            assert!((c - a) < 0.00001 && (a - c) < 0.00001);
            for _ in 0..1000000000 {
                unsafe { asm!("pause") };
            }
        }
        1 => unsafe {
            let addr: usize = PF_ADDR;
            let ptr: *mut u64 = core::mem::transmute(addr);
            *ptr = 1;
        },
        _ => {}
    }
    exit(Ok(12345i32));
}

fn join(normal: Handle, fault: Handle) {
    log::trace!("join: normal = {:?}, fault = {:?}", normal, fault);

    let ret = task_join(normal);
    assert_eq!(ret, Ok(12345));

    let ret = task_join(fault);
    assert_eq!(ret, Err(crate::Error::EFAULT));
}

fn sleep() {
    log::trace!("sleep");
    task_sleep(50).expect("Failed to sleep");
}

fn debug_mem(st: Handle) {
    log::trace!("debug_mem: st = {:?}", st);

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
    assert_eq!(ret, Err(crate::Error::EPERM));
}

fn debug_reg_gpr(st: Handle) {
    log::trace!("debug_reg_gpr: st = {:?}", st);

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
    log::trace!("debug_reg_fpu: st = {:?}", st);

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

fn debug_excep(task: Handle, st: Handle) {
    log::trace!("debug_reg_excep: task = {:?}, st = {:?}", task, st);
    let chan = {
        let mut chan = Handle::NULL;
        task_debug(
            st,
            TASK_DBG_EXCEP_HDL,
            0,
            (&mut chan as *mut Handle).cast(),
            size_of::<Handle>(),
        )
        .expect("Failed to create exception channel");
        obj_drop(st).expect("Failed to resume the task");
        chan
    };

    let mut hdl_buf = [Handle::NULL; 0];
    let mut excep = MaybeUninit::<Exception>::uninit();
    let mut packet = crate::ipc::RawPacket {
        handles: hdl_buf.as_mut_ptr(),
        handle_count: 0,
        handle_cap: hdl_buf.len(),
        buffer: excep.as_mut_ptr().cast(),
        buffer_size: size_of::<Exception>(),
        buffer_cap: size_of::<Exception>(),
    };
    chan_recv(chan, &mut packet, u64::MAX).expect("Failed to receive exception");
    let excep = unsafe { excep.assume_init() };
    assert_eq!(excep.cr2, PF_ADDR as u64);

    let exres = MaybeUninit::<ExceptionResult>::new(ExceptionResult { code: 0 });
    packet.buffer = exres.as_ptr().cast::<u8>() as *mut _;
    packet.buffer_size = size_of::<ExceptionResult>();
    packet.buffer_cap = size_of::<ExceptionResult>();
    chan_send(chan, &packet).expect("Failed to send exception result");

    let ret = task_join(task);
    assert_eq!(ret, Err(crate::Error::EFAULT));
}

fn suspend(task: Handle) {
    log::trace!("suspend: task = {:?}", task);

    let mut st = Handle::NULL;

    task_ctl(task, TASK_CTL_SUSPEND, &mut st).expect("Failed to suspend a task");
    sleep();

    debug_mem(st);
    debug_reg_gpr(st);
    debug_reg_fpu(st);

    obj_drop(st).expect("Failed to resume the task");
}

fn kill(task: Handle) {
    log::trace!("kill: task = {:?}", task);

    task_ctl(task, TASK_CTL_KILL, null_mut()).expect("Failed to kill a task");

    let ret = task_join(task);
    assert_eq!(ret, Err(crate::Error::EKILLED));
}

fn ctl(task: Handle) {
    log::trace!("ctl: task = {:?}", task);
    suspend(task);
    sleep();
    kill(task);
}

pub fn test() {
    // Test the defence of invalid user pointer access.
    let ret = task_fn(
        0x100000000 as *const CreateInfo,
        CreateFlags::empty(),
        null_mut(),
    );
    assert_eq!(ret, Err(crate::Error::EPERM));

    let creator = |arg: u32, cf: Option<CreateFlags>, extra: *mut crate::Handle| {
        let mut c1 = Handle::NULL;
        let mut c2 = Handle::NULL;
        chan_new(&mut c1, &mut c2).expect("Failed to create channel");
        obj_drop(c1).expect("Failed to drop channel");
        let ci = CreateInfo {
            name: null_mut(),
            name_len: 0,
            stack_size: crate::task::DEFAULT_STACK_SIZE,
            init_chan: c2,
            func: func as *mut u8,
            arg: arg as *mut u8,
        };
        task_fn(&ci, cf.unwrap_or(CreateFlags::empty()), extra)
    };

    join(
        creator(100, None, null_mut()).expect("Failed to create task"),
        creator(1, None, null_mut()).expect("Failed to create task"),
    );

    ctl(creator(0, None, null_mut()).expect("Failed to create task"));

    let mut st = Handle::NULL;
    let task =
        creator(1, Some(CreateFlags::SUSPEND_ON_START), &mut st).expect("Failed to create task");
    debug_excep(task, st);
}
