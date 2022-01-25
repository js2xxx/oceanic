use core::hint;

use super::*;
use crate::{cpu::CpuLocalLazy, mem::space, sched::deque};

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
    CpuLocalLazy::new(deque::Injector::new);

#[thread_local]
pub(super) static IDLE: CpuLocalLazy<Tid> = CpuLocalLazy::new(|| {
    let cpu = unsafe { crate::cpu::id() };

    let ti = TaskInfo::builder()
        .from(None)
        .name(format!("IDLE{}", cpu))
        .ty(Type::Kernel)
        .affinity(crate::cpu::current_mask())
        .build()
        .unwrap();

    let space = Arc::clone(unsafe { space::current() });

    let stack = space
        .init_stack(DEFAULT_STACK_SIZE)
        .expect("Failed to initialize stack for IDLE");

    let entry = create_entry(
        LAddr::new(idle as *mut u8),
        stack,
        [cpu as u64, unsafe {
            archop::msr::read(archop::msr::FS_BASE)
        }],
    );
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

    let (_, ctx_chan) = Channel::new();
    let ctx_chan = unsafe { Ref::new(ctx_chan).coerce_unchecked() };

    let space = SCHED
        .with_current(|cur| Ok(Arc::clone(cur.space())))
        .expect("Failed to clone current space");
    let stack = space
        .init_stack(DEFAULT_STACK_SIZE)
        .expect("Failed to initialize stack");

    let (ctx_dropper, ..) = task::create(
        Some(String::from("CTXD")),
        space,
        LAddr::new(ctx_dropper as *mut u8),
        stack,
        ctx_chan,
        unsafe { archop::msr::read(archop::msr::FS_BASE) },
    )
    .expect("Failed to create context dropper");
    SCHED.unblock(ctx_dropper);

    if cpu == 0 {
        let (me, chan) = Channel::new();
        let chan = unsafe { Ref::new(chan).coerce_unchecked() };

        me.send(&mut crate::sched::ipc::Packet::new(
            hdl::List::default(),
            &[],
        ))
        .expect("Failed to send message");

        let image = unsafe {
            core::slice::from_raw_parts(
                *crate::kargs().tinit_phys.to_laddr(minfo::ID_OFFSET),
                crate::kargs().tinit_len,
            )
        };

        let (tinit, ..) =
            task::from_elf(image, String::from("TINIT"), crate::cpu::all_mask(), chan)
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
        let _ = crate::sched::SCHED.with_current(|cur| {
            cur.running_state = RunningState::NEED_RESCHED;
            Ok(())
        });
        unsafe { archop::resume_intr(None) };
    }
}
