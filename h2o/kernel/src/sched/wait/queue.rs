use crossbeam_queue::SegQueue;

use super::WaitObject;

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

    pub fn pop(&self, block_desc: &'static str) -> T {
        loop {
            if let Some(data) = self.data.pop() {
                break data;
            }
            self.wo.wait((), block_desc);
        }
    }

    pub fn try_pop(&self) -> Option<T> {
        self.data.pop()
    }

    pub fn push(&self, data: T) {
        self.data.push(data);
        self.wo.notify(None);
    }
}
