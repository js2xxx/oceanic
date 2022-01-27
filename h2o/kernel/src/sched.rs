mod imp;
pub mod ipc;
pub mod task;
pub mod wait;

pub use self::imp::{deque, epoch, task_migrate_handler, Scheduler, PREEMPT, SCHED};

#[inline]
pub fn init() {
    task::init();
}
