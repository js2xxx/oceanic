pub mod cell;

pub use cell::WaitCell;

use super::*;
use crate::cpu::time::Instant;

#[derive(Debug)]
pub struct WaitObject {
    pub(super) wait_queue: deque::Injector<task::Blocked>,
}

impl WaitObject {
    pub fn new() -> Self {
        WaitObject {
            wait_queue: deque::Injector::new(),
        }
    }

    pub fn wait<T>(&self, guard: T, block_desc: &'static str) {
        SCHED.block_current(Instant::now(), guard, self, block_desc);
    }

    pub fn notify(&self, num: Option<usize>) -> usize {
        let num = num.unwrap_or(usize::MAX);

        let mut cnt = 0;
        while cnt < num {
            match self.wait_queue.steal() {
                deque::Steal::Success(task) => {
                    SCHED.unblock(task);
                    cnt += 1;
                }
                deque::Steal::Retry => {}
                deque::Steal::Empty => break,
            }
        }
        cnt
    }
}
