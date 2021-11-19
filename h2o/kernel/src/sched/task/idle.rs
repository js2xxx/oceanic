use core::{any::Any, hint};

use spin::Lazy;

use super::*;
use crate::{
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
pub(super) static CTX_DROPPER: Lazy<deque::Injector<Box<dyn Any>>> =
    Lazy::new(|| deque::Injector::new());

#[thread_local]
pub(super) static IDLE: Lazy<Tid> = Lazy::new(|| {
    let cpu = unsafe { crate::cpu::id() };

    let ti = TaskInfo {
        from: Some((*ROOT, UserHandle::NULL)),
        name: format!("IDLE{}", cpu),
        ty: Type::Kernel,
        affinity: crate::cpu::current_mask(),
        prio: prio::IDLE,
        user_handles: UserHandles::new(),
    };

    let space = Space::clone(unsafe { space::current() }, Type::Kernel);
    let entry = LAddr::new(idle as *mut u8);

    let init = Init::new(
        ti,
        space,
        entry,
        DEFAULT_STACK_SIZE,
        Some(paging::LAddr::from(
            unsafe { archop::msr::read(archop::msr::FS_BASE) } as usize,
        )),
        [cpu as u64, 0],
    )
    .expect("Failed to initialize IDLE");
    let tid = init.tid;

    crate::sched::SCHED.push(init);
    tid
});

fn idle(cpu: usize) -> ! {
    use crate::sched::{task, SCHED};
    log::debug!("IDLE #{}", cpu);

    if cpu == 0 {
        let image = unsafe {
            core::slice::from_raw_parts(
                *crate::KARGS.tinit_phys.to_laddr(minfo::ID_OFFSET),
                crate::KARGS.tinit_len,
            )
        };

        let (tinit, ..) =
            task::from_elf(image, String::from("TINIT"), crate::cpu::all_mask(), [0, 0])
                .expect("Failed to initialize TINIT");
        SCHED.push(tinit);
    }
    let (ctx_dropper, ..) = task::create_fn(
        Some(String::from("CTXD")),
        DEFAULT_STACK_SIZE,
        LAddr::new(ctx_dropper as *mut u8),
        unsafe { archop::msr::read(archop::msr::FS_BASE) } as *mut u8,
    )
    .expect("Failed to create context dropper");
    SCHED.push(ctx_dropper);

    unsafe { archop::halt_loop(None) };
}

fn ctx_dropper(fs_base: u64) -> ! {
    log::debug!("Context dropper for cpu #{}", unsafe { crate::cpu::id() });
    unsafe { archop::msr::write(archop::msr::FS_BASE, fs_base) };

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
    }
}
