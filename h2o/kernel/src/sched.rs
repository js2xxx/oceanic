mod imp;
pub mod ipc;
pub mod task;
pub mod wait;

pub use self::imp::{deque, epoch};
pub(crate) use self::{
    imp::{task_migrate_handler, waiter::Blocker, PREEMPT, SCHED},
    ipc::{basic::BasicEvent, *},
};

#[inline]
pub fn init() {
    task::init();
}
