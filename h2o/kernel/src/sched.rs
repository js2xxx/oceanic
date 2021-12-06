pub mod ipc;
mod sched;
pub mod task;
pub mod wait;

pub use sched::{deque, epoch, task_migrate_handler, Scheduler, PREEMPT, SCHED};

pub fn init() {
    task::init();
}
