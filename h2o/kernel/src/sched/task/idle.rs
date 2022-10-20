use core::hint;

use super::*;
use crate::{cpu::Lazy, mem::space, sched::deque};

/// Context dropper - used for dropping kernel stacks of threads.
///
/// The function [`task_exit`] runs on its own thread stack, so we cannot drop
/// its kernel stack immediately, or the current context will crash. That's
/// where context dropper tasks come into play. They collect the kernel stack
/// from its CPU-local queue and drop it.
///
/// [`task_exit`]: crate::sched::task::syscall::task_exit
#[thread_local]
pub(super) static CTX_DROPPER: Lazy<deque::Injector<alloc::boxed::Box<Context>>> =
    Lazy::new(deque::Injector::new);

#[thread_local]
pub(super) static IDLE: Lazy<Tid> = Lazy::new(|| {
    let cpu = unsafe { crate::cpu::id() };

    let ti = TaskInfo::builder()
        .from(Default::default())
        .excep_chan(Arsc::try_new(Default::default()).expect("Failed to create task info"))
        .name(format!("IDLE{}", cpu))
        .ty(Type::Kernel)
        .affinity(crate::cpu::current_mask())
        .build()
        .unwrap();

    let space = super::Space::new_current();
    let stack = space::init_stack(space.mem(), DEFAULT_STACK_SIZE)
        .expect("Failed to initialize stack for IDLE");

    let entry = ctx::Entry {
        entry: LAddr::new(idle as *mut u8),
        stack,
        args: [cpu as u64, unsafe { archop::reg::read_fs() }],
    };
    let kstack = ctx::Kstack::new(Some(entry), Type::Kernel);

    let tid = tid::allocate(ti).expect("Tid exhausted");
    space.set_main(&tid);

    let init = Init::new(tid.clone(), space, kstack, ctx::ExtFrame::zeroed());
    crate::sched::SCHED.unblock(init, true);

    tid
});

fn idle(cpu: usize, fs_base: u64) -> ! {
    unsafe { archop::reg::write_fs(fs_base) };

    log::debug!("IDLE #{}", cpu);

    if cpu == 0 {
        boot::setup();
    }

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
