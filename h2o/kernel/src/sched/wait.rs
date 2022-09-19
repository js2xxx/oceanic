mod futex;

use core::time::Duration;

use crossbeam_queue::SegQueue;

pub use self::futex::*;
use super::{ipc::Arsc, *};
use crate::cpu::time::Timer;

#[derive(Debug)]
pub struct WaitObject {
    pub(super) wait_queue: SegQueue<Arsc<Timer>>,
}

unsafe impl Send for WaitObject {}
unsafe impl Sync for WaitObject {}

impl WaitObject {
    #[inline]
    pub fn new() -> Self {
        WaitObject {
            wait_queue: SegQueue::new(),
        }
    }

    #[inline]
    pub fn wait<T>(
        &self,
        guard: T,
        timeout: Duration,
        block_desc: &'static str,
    ) -> sv_call::Result {
        let timer = SCHED.block_current(guard, Some(&self.wait_queue), timeout, block_desc);
        timer.and_then(|timer| {
            if !timer.is_fired() {
                Ok(())
            } else {
                Err(sv_call::ETIME)
            }
        })
    }

    pub fn notify(&self, num: usize, preempt: bool) -> usize {
        let num = if num == 0 { usize::MAX } else { num };

        let mut cnt = 0;
        while cnt < num {
            match self.wait_queue.pop() {
                Some(timer) if timer.cancel(preempt) => {
                    cnt += 1;
                }
                Some(_) => {}
                None => break,
            }
        }
        cnt
    }
}

impl Default for WaitObject {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
