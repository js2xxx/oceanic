pub mod queue;
pub mod cell;

pub use cell::WaitCell;

use super::*;

use alloc::collections::LinkedList;
use alloc::sync::Arc;
use spin::Mutex;

#[derive(Debug)]
pub struct WaitObject {
      pub(super) wait_queue: Mutex<LinkedList<task::Blocked>>,
}

impl WaitObject {
      pub fn new() -> Arc<Self> {
            Arc::new(WaitObject {
                  wait_queue: Mutex::new(LinkedList::new()),
            })
      }

      pub fn wait<T>(self: &Arc<Self>, guard: T, block_desc: &'static str) {
            if let Some(cur) = SCHED.lock().current_mut() {
                  cur.running_state = task::RunningState::Drowsy(self.clone(), block_desc);
                  drop(guard);
            }
            crate::cpu::time::delay(core::time::Duration::from_nanos(1_00));
            
      }

      pub fn notify(self: &Arc<Self>, num: Option<usize>) -> usize {
            let num = num.unwrap_or(usize::MAX);
            let mut wait_queue = self.wait_queue.lock();

            let mut cnt = 0;
            while cnt < num {
                  if let Some(task) = wait_queue.pop_front() {
                        SCHED.lock().unblock(task);
                        cnt += 1;
                  } else {
                        break;
                  }
            }
            cnt
      }
}
