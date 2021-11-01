pub mod cell;

use alloc::sync::Arc;

pub use cell::WaitCell;

use super::*;

#[derive(Debug)]
pub struct WaitObject {
    pub(super) wait_queue: deque::Injector<task::Blocked>,
}

impl WaitObject {
    pub fn new() -> Arc<Self> {
        Arc::new(WaitObject {
            wait_queue: deque::Injector::new(),
        })
    }

    pub fn wait<T>(self: &Arc<Self>, guard: T, block_desc: &'static str) {
        SCHED.with_current(|cur| {
            cur.running_state = task::RunningState::Drowsy(self.clone(), block_desc);
            drop(guard);
        });
        // TODO: Find a more reasonable way to strike into the interrupt.
        unsafe { asm!("int 32") };
    }

    pub fn notify(self: &Arc<Self>, num: Option<usize>) -> usize {
        let num = num.unwrap_or(usize::MAX);

        let mut cnt = 0;
        while cnt < num {
            match self.wait_queue.steal() {
                deque::Steal::Success(task) => {
                    super::SCHED.unblock(task);
                    cnt += 1;
                }
                deque::Steal::Retry => {}
                deque::Steal::Empty => break,
            }
        }
        cnt
    }
}
