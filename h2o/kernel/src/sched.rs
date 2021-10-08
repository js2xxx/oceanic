pub mod sched;
pub mod task;
pub mod wait;

pub use sched::{deque, epoch, Scheduler, SCHED};

pub fn init() {
      task::init();
}
