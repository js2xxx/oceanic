use super::*;
use crate::cpu::time::Instant;

use alloc::collections::LinkedList;
use alloc::string::String;
use spin::Mutex;

pub struct WaitObject {
      wait_queue: Mutex<LinkedList<task::Blocked>>,
}

impl WaitObject {
      pub fn new() -> Self {
            WaitObject {
                  wait_queue: Mutex::new(LinkedList::new()),
            }
      }

      pub fn wait<T>(&self, guard: T, block_desc: String) {
            let task = SCHED
                  .lock()
                  .block_current(Instant::now(), block_desc)
                  .expect("Current task disappeared");
            self.wait_queue.lock().push_back(task);
            drop(guard);
      }

      pub fn notify(&self) -> usize {
            let mut wait_queue = self.wait_queue.lock();
            let len = wait_queue.len();
            while let Some(task) = wait_queue.pop_front() {
                  SCHED.lock().unblock(task);
            }
            len
      }
}
