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
use crate::{call::*, mem::Flags, task::excep::ExceptionResult};

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
        id: 0,
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

pub fn test() -> (*mut u8, *mut u8, Handle) {
    // Test the defence of invalid user pointer access.
    let ret = task_exec(0x100000000 as *const ExecInfo);
    assert_eq!(ret, Err(crate::Error::EPERM));

    let flags = Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS;
    let stack_phys =
        phys_alloc(DEFAULT_STACK_SIZE, 4096, flags.bits()).expect("Failed to allocate memory");
    let mi = crate::mem::MapInfo {
        addr: 0,
        map_addr: false,
        phys: stack_phys,
        phys_offset: 0,
        len: DEFAULT_STACK_SIZE,
        flags,
    };
    let stack_base = mem_map(Handle::NULL, &mi).expect("Failed to map memory");
    let stack_ptr = unsafe { stack_base.add(DEFAULT_STACK_SIZE - 4096) };

    let creator = |arg: u32| {
        let mut c1 = Handle::NULL;
        let mut c2 = Handle::NULL;
        chan_new(&mut c1, &mut c2).expect("Failed to create channel");
        obj_drop(c1).expect("Failed to drop channel");
        let ci = ExecInfo {
            name: null_mut(),
            name_len: 0,
            space: Handle::NULL,
            entry: func as *mut u8,
            stack: stack_ptr,
            init_chan: c2,
            arg: arg.into(),
        };
        task_exec(&ci)
    };

    join(
        creator(100).expect("Failed to create task"),
        creator(1).expect("Failed to create task"),
    );

    ctl(creator(0).expect("Failed to create task"));

    let mut st = Handle::NULL;
    let task = {
        let t = task_new(null_mut(), 0, Handle::NULL, &mut st).expect("Failed to create task");
        let frame = Gpr {
            rip: func as usize as u64,
            rsp: stack_ptr as u64,
            rflags: 1 << 9,
            rdi: 0,
            rsi: 1,
            ..Default::default()
        };
        task_debug(
            st,
            TASK_DBG_WRITE_REG,
            TASK_DBGADDR_GPR,
            (&frame as *const Gpr) as *mut u8,
            core::mem::size_of::<Gpr>(),
        )
        .expect("Failed to write task's data");
        t
    };
    debug_excep(task, st);

    (stack_ptr, stack_base, stack_phys)
}
