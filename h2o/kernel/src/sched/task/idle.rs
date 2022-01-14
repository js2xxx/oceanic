use alloc::vec::Vec;
use core::hint;

use super::*;
use crate::{
    cpu::CpuLocalLazy,
    mem::space::{self, Space},
    sched::deque,
};

/// Context dropper - used for dropping kernel stacks of threads.
///
/// The function [`task_exit`] runs on its own thread stack, so we cannot drop
/// its kernel stack immediately, or the current context will crash. That's
/// where context dropper tasks come into play. They collect the kernel stack
/// from its CPU-local queue and drop it.
///
/// [`task_exit`]: crate::sched::task::syscall::task_exit
#[thread_local]
pub(super) static CTX_DROPPER: CpuLocalLazy<deque::Injector<alloc::boxed::Box<Context>>> =
    CpuLocalLazy::new(|| deque::Injector::new());

#[thread_local]
pub(super) static IDLE: CpuLocalLazy<Tid> = CpuLocalLazy::new(|| {
    let cpu = unsafe { crate::cpu::id() };

    let ti = TaskInfo::builder()
        .from(Some(ROOT.clone()))
        .name(format!("IDLE{}", cpu))
        .ty(Type::Kernel)
        .affinity(crate::cpu::current_mask())
        .prio(prio::IDLE)
        .build()
        .unwrap();

    let space = Space::clone(unsafe { space::current() }, Type::Kernel);

    let entry = create_entry(
        &space,
        LAddr::new(idle as *mut u8),
        DEFAULT_STACK_SIZE,
        [cpu as u64, unsafe {
            archop::msr::read(archop::msr::FS_BASE)
        }],
    )
    .expect("Failed to initialize IDLE");
    let kstack = ctx::Kstack::new(entry, Type::Kernel);

    let tid = tid::allocate(ti).expect("Tid exhausted");

    let init = Init::new(tid.clone(), space, kstack, ctx::ExtFrame::zeroed());
    crate::sched::SCHED.unblock(init);

    tid
});

fn idle(cpu: usize, fs_base: u64) -> ! {
    unsafe { archop::msr::write(archop::msr::FS_BASE, fs_base) };

    use crate::sched::{task, SCHED};
    log::debug!("IDLE #{}", cpu);

    let (ctx_dropper, ..) = task::create_fn(
        Some(String::from("CTXD")),
        Some(Type::Kernel),
        None,
        None,
        LAddr::new(ctx_dropper as *mut u8),
        None,
        unsafe { archop::msr::read(archop::msr::FS_BASE) },
        DEFAULT_STACK_SIZE,
    )
    .expect("Failed to create context dropper");
    SCHED.unblock(ctx_dropper);

    if cpu == 0 {
        let (me, chan) = Channel::new();

        me.send(crate::sched::ipc::Packet::new(Vec::new(), &[]))
            .expect("Failed to send message");

        let image = unsafe {
            core::slice::from_raw_parts(
                *crate::kargs().tinit_phys.to_laddr(minfo::ID_OFFSET),
                crate::kargs().tinit_len,
            )
        };

        let (tinit, ..) = task::from_elf(
            image,
            String::from("TINIT"),
            crate::cpu::all_mask(),
            Some(chan),
        )
        .expect("Failed to initialize TINIT");
        SCHED.unblock(tinit);
    }

    unsafe { archop::halt_loop(Some(true)) };
}

fn ctx_dropper(_: u64, fs_base: u64) -> ! {
    unsafe { archop::msr::write(archop::msr::FS_BASE, fs_base) };
    log::debug!("Context dropper for cpu #{}", unsafe { crate::cpu::id() });

    let worker = deque::Worker::new_fifo();
    loop {
        match CTX_DROPPER.steal_batch(&worker) {
            deque::Steal::Empty | deque::Steal::Retry => hint::spin_loop(),
            deque::Steal::Success(_) => {
                while let Some(obj) = worker.pop() {
                    drop(obj);
                }
            }
        }
        crate::sched::SCHED.with_current(|cur| cur.running_state = RunningState::NEED_RESCHED);
        unsafe { archop::resume_intr(None) };
    }
}
