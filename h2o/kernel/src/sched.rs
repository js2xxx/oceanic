pub mod sched;
pub mod task;
pub mod wait;

pub use sched::{Scheduler, SCHED};

pub fn init() {
      task::init();
}
