use core::time::Duration;

use crossbeam_queue::SegQueue;

use super::WaitObject;

#[derive(Debug)]
pub struct WaitQueue<T> {
    data: SegQueue<T>,
    wo: WaitObject,
}

impl<T> WaitQueue<T> {
    pub fn new() -> Self {
        WaitQueue {
            data: SegQueue::new(),
            wo: WaitObject::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn pop(&self, timeout: Duration, block_desc: &'static str) -> Option<T> {
        loop {
            if let Some(data) = self.data.pop() {
                break Some(data);
            }
            if !self.wo.wait((), timeout, block_desc) {
                break None;
            }
        }
    }

    pub fn try_pop(&self) -> Option<T> {
        self.data.pop()
    }

    pub fn push(&self, data: T) {
        self.data.push(data);
        self.wo.notify(1);
    }
}

impl<T> Default for WaitQueue<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
