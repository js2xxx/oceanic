use core::{
    arch::asm,
    mem::{size_of, MaybeUninit},
    ptr::null_mut,
};

use solvent::prelude::{Object, Phys, Virt};
use sv_call::{
    ipc::{RawPacket, SIG_READ},
    mem::Flags,
    task::{
        ctx::{Gpr, GPR_SIZE},
        excep::{Exception, ExceptionResult},
        *,
    },
    *,
};

const PF_ADDR: usize = 0x1598_0000_0000;

unsafe extern "C" fn func(_: Handle, arg: u32) {
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
            let ptr = PF_ADDR as *mut u64;
            *ptr = 1;
        },
        _ => {}
    }
    sv_task_exit(12345, false)
        .into_res()
        .expect("Failed to exit the task");
}

unsafe fn join(normal: Handle, fault: Handle) {
    log::trace!("join: normal = {:?}, fault = {:?}", normal, fault);
    let mut ret = Default::default();

    sv_obj_wait(normal, u64::MAX, true, false, SIG_READ)
        .into_res()
        .expect("Failed to wait for the task");
    sv_task_join(normal, &mut ret)
        .into_res()
        .expect("Failed to join the task");
    assert_eq!(ret, 12345);

    sv_obj_wait(fault, u64::MAX, true, false, SIG_READ)
        .into_res()
        .expect("Failed to wait for the task");
    sv_task_join(fault, &mut ret)
        .into_res()
        .expect("Failed to join the task");
    assert_eq!(Error::try_from_retval(ret), Some(EFAULT));
}

unsafe fn sleep() {
    log::trace!("sleep");
    sv_task_sleep(50).into_res().expect("Failed to sleep");
}

unsafe fn debug_mem(st: Handle) {
    log::trace!("debug_mem: st = {:?}", st);

    let mut buf = [0u8; 15];
    sv_task_debug(st, TASK_DBG_READ_MEM, 0x401000, buf.as_mut_ptr(), buf.len())
        .into_res()
        .expect("Failed to read memory");
    let ret = sv_task_debug(
        st,
        TASK_DBG_WRITE_MEM,
        0x401000,
        buf.as_mut_ptr(),
        buf.len(),
    );
    assert_eq!(ret.into_res(), Err(EPERM));
}

unsafe fn debug_reg_gpr(st: Handle) {
    log::trace!("debug_reg_gpr: st = {:?}", st);

    let mut buf = MaybeUninit::<Gpr>::uninit();
    sv_task_debug(
        st,
        TASK_DBG_READ_REG,
        TASK_DBGADDR_GPR,
        buf.as_mut_ptr().cast(),
        GPR_SIZE,
    )
    .into_res()
    .expect("Failed to read general registers");
    sv_task_debug(
        st,
        TASK_DBG_WRITE_REG,
        TASK_DBGADDR_GPR,
        buf.as_mut_ptr().cast(),
        GPR_SIZE,
    )
    .into_res()
    .expect("Failed to write general registers");
}

unsafe fn debug_reg_fpu(st: Handle) {
    log::trace!("debug_reg_fpu: st = {:?}", st);

    let mut buf = [0u8; 576];
    sv_task_debug(
        st,
        TASK_DBG_READ_REG,
        TASK_DBGADDR_FPU,
        buf.as_mut_ptr(),
        buf.len(),
    )
    .into_res()
    .expect("Failed to read FPU registers");
    sv_task_debug(
        st,
        TASK_DBG_WRITE_REG,
        TASK_DBGADDR_FPU,
        buf.as_mut_ptr(),
        buf.len(),
    )
    .into_res()
    .expect("Failed to write FPU registers");
}

unsafe fn debug_excep(task: Handle, st: Handle) {
    log::trace!("debug_reg_excep: task = {:?}, st = {:?}", task, st);
    let chan = {
        let mut chan = Handle::NULL;
        sv_task_debug(
            st,
            TASK_DBG_EXCEP_HDL,
            0,
            (&mut chan as *mut Handle).cast(),
            size_of::<Handle>(),
        )
        .into_res()
        .expect("Failed to create exception channel");
        sv_obj_drop(st)
            .into_res()
            .expect("Failed to resume the task");
        chan
    };

    let mut hdl_buf = [Handle::NULL; 0];
    let mut excep = MaybeUninit::<Exception>::uninit();
    let mut packet = RawPacket {
        id: 0,
        handles: hdl_buf.as_mut_ptr(),
        handle_count: 0,
        handle_cap: hdl_buf.len(),
        buffer: excep.as_mut_ptr().cast(),
        buffer_size: size_of::<Exception>(),
        buffer_cap: size_of::<Exception>(),
    };
    sv_obj_wait(chan, u64::MAX, true, false, SIG_READ)
        .into_res()
        .expect("Failed to wait for the channel");
    sv_chan_recv(chan, &mut packet)
        .into_res()
        .expect("Failed to receive exception");
    let excep = unsafe { excep.assume_init() };
    assert_eq!(excep.cr2, PF_ADDR as u64);

    let exres = MaybeUninit::<ExceptionResult>::new(ExceptionResult { code: 0 });
    packet.buffer = exres.as_ptr().cast::<u8>() as *mut _;
    packet.buffer_size = size_of::<ExceptionResult>();
    packet.buffer_cap = size_of::<ExceptionResult>();
    sv_chan_send(chan, &packet)
        .into_res()
        .expect("Failed to send exception result");

    sv_obj_wait(task, u64::MAX, true, false, SIG_READ)
        .into_res()
        .expect("Failed to wait for the task");
    let mut ret = Default::default();
    sv_task_join(task, &mut ret)
        .into_res()
        .expect("Failed to join the task");
    assert_eq!(Error::try_from_retval(ret), Some(EFAULT));
}

unsafe fn suspend(task: Handle) {
    log::trace!("suspend: task = {:?}", task);

    let mut st = Handle::NULL;

    sv_task_ctl(task, TASK_CTL_SUSPEND, &mut st)
        .into_res()
        .expect("Failed to suspend a task");
    sleep();

    debug_mem(st);
    debug_reg_gpr(st);
    debug_reg_fpu(st);

    sv_obj_drop(st)
        .into_res()
        .expect("Failed to resume the task");
}

unsafe fn kill(task: Handle) {
    log::trace!("kill: task = {:?}", task);

    sv_task_ctl(task, TASK_CTL_KILL, null_mut())
        .into_res()
        .expect("Failed to kill a task");

    sv_obj_wait(task, u64::MAX, true, false, SIG_READ)
        .into_res()
        .expect("Failed to wait for the task");
    let mut ret = Default::default();
    sv_task_join(task, &mut ret)
        .into_res()
        .expect("Failed to join the task");
    assert_eq!(Error::try_from_retval(ret), Some(EKILLED));
}

unsafe fn ctl(task: Handle) {
    log::trace!("ctl: task = {:?}", task);
    suspend(task);
    sleep();
    kill(task);
}

pub unsafe fn test(virt: &Virt) -> (*mut u8, *mut u8, Handle) {
    // Test the defence of invalid user pointer access.
    let ret = sv_task_exec(0x100000000 as *const ExecInfo);
    assert_eq!(ret.into_res(), Err(EPERM));

    let flags = Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS;
    let stack_phys = sv_phys_alloc(DEFAULT_STACK_SIZE, Default::default())
        .into_res()
        .expect("Failed to allocate memory");
    let stack_phys2 = sv_obj_clone(stack_phys)
        .into_res()
        .expect("Failed to clone stack");
    let stack_base = virt
        .map_phys(None, Phys::from_raw(stack_phys), flags)
        .expect("Failed to map memory")
        .as_mut_ptr();
    let stack_ptr = unsafe { stack_base.add(DEFAULT_STACK_SIZE - 4096) };

    let creator = |arg: u32| {
        let mut c1 = Handle::NULL;
        let mut c2 = Handle::NULL;
        sv_chan_new(&mut c1, &mut c2)
            .into_res()
            .expect("Failed to create channel");
        sv_obj_drop(c1).into_res().expect("Failed to drop channel");
        let ci = ExecInfo {
            name: null_mut(),
            name_len: 0,
            space: Handle::NULL,
            entry: func as *mut u8,
            stack: stack_ptr,
            init_chan: c2,
            arg: arg.into(),
        };
        sv_task_exec(&ci)
    };

    join(
        creator(100).into_res().expect("Failed to create task"),
        creator(1).into_res().expect("Failed to create task"),
    );

    ctl(creator(0).into_res().expect("Failed to create task"));

    let mut st = Handle::NULL;
    let task = {
        let t = sv_task_new(null_mut(), 0, Handle::NULL, Handle::NULL, &mut st)
            .into_res()
            .expect("Failed to create task");
        let frame = Gpr {
            rip: func as usize as u64,
            rsp: stack_ptr as u64,
            rflags: 1 << 9,
            rdi: 0,
            rsi: 1,
            ..Default::default()
        };
        sv_task_debug(
            st,
            TASK_DBG_WRITE_REG,
            TASK_DBGADDR_GPR,
            (&frame as *const Gpr) as *mut u8,
            core::mem::size_of::<Gpr>(),
        )
        .into_res()
        .expect("Failed to write task's data");
        t
    };
    debug_excep(task, st);

    (stack_ptr, stack_base, stack_phys2)
}
