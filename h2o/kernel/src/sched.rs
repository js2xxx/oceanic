pub mod task;
pub mod sched;

pub use sched::SCHED;

pub fn init() {
      task::init();
}