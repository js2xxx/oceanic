pub mod task;
pub mod sched;
pub mod wait;

pub use sched::SCHED;

pub fn init() {
      task::init();
}