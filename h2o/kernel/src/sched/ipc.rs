use alloc::sync::{Arc, Weak};

use super::{task::TaskError, wait::WaitQueue};

pub const MAX_BUFFER_SIZE: usize = paging::PAGE_SIZE;
pub const MAX_QUEUE_SIZE: usize = 2048;

#[derive(Debug)]
pub enum IpcError<T> {
    QueueFull(T),
    QueueEmpty,
    Task(TaskError),
    ChannelClosed(T),
}

pub struct Channel<T> {
    me: Arc<WaitQueue<T>>,
    peer: Weak<WaitQueue<T>>,
}

impl<T> Channel<T> {
    pub fn new() -> (Self, Self) {
        let q1 = Arc::new(WaitQueue::new());
        let q2 = Arc::new(WaitQueue::new());
        let c1 = Channel {
            me: q1.clone(),
            peer: Arc::downgrade(&q2),
        };
        let c2 = Channel {
            me: q2,
            peer: Arc::downgrade(&q1),
        };
        (c1, c2)
    }

    pub fn send(&self, msg: T) -> Result<(), IpcError<T>> {
        match self.peer.upgrade() {
            None => Err(IpcError::ChannelClosed(msg)),
            Some(peer) => {
                if peer.len() >= MAX_QUEUE_SIZE {
                    Err(IpcError::QueueFull(msg))
                } else {
                    peer.push(msg);
                    Ok(())
                }
            }
        }
    }

    pub fn receive(&self) -> Result<T, IpcError<T>> {
        Ok(self.me.pop("Channel::receive"))
    }

    pub fn try_receive(&self) -> Result<T, IpcError<T>> {
        self.me.try_pop().ok_or(IpcError::QueueEmpty)
    }
}
