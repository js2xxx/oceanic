pub mod ipc;
mod imp;
pub mod task;
pub mod wait;

pub use imp::{deque, epoch, task_migrate_handler, Scheduler, PREEMPT, SCHED};

pub fn init() {
    task::init();
}
