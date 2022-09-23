mod syscall;

use alloc::sync::{Arc, Weak};
use core::{
    mem,
    sync::atomic::{AtomicU64, Ordering::SeqCst},
};

use bytes::Bytes;
use crossbeam_queue::SegQueue;
use spin::Mutex;
use sv_call::Feature;

use super::{Event, SIG_READ};
use crate::sched::{
    task::hdl::{self, DefaultFeature},
    BasicEvent, PREEMPT, SCHED,
};

const MAX_QUEUE_SIZE: usize = 2048;

#[derive(Debug, Default)]
pub struct Packet {
    id: usize,
    objects: hdl::List,
    buffer: Bytes,
}

unsafe impl Send for Packet {}
unsafe impl Sync for Packet {}

impl Packet {
    pub fn new(id: usize, objects: hdl::List, data: &[u8]) -> Self {
        let buffer = Bytes::copy_from_slice(data);
        Packet {
            id,
            objects,
            buffer,
        }
    }

    #[inline]
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    #[inline]
    pub fn buffer_mut(&mut self) -> &mut Bytes {
        &mut self.buffer
    }

    #[inline]
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }
}


#[derive(Debug)]
struct ChannelSide {
    msgs: SegQueue<Packet>,
    event: Arc<BasicEvent>,
}

impl Default for ChannelSide {
    #[inline]
    fn default() -> Self {
        ChannelSide {
            msgs: SegQueue::new(),
            event: BasicEvent::new(0),
        }
    }
}

#[derive(Debug)]
pub struct Channel {
    peer_id: u64,
    me: Arc<ChannelSide>,
    peer: Weak<ChannelSide>,
    head: Mutex<Option<Packet>>,
}

impl Channel {
    pub fn new() -> (Self, Self) {
        static PEER_ID: AtomicU64 = AtomicU64::new(0);
        let peer_id = PEER_ID.fetch_add(1, SeqCst);

        let q1 = Arc::new(ChannelSide::default());
        let q2 = Arc::new(ChannelSide::default());
        let c1 = Channel {
            peer_id,
            me: Arc::clone(&q1),
            peer: Arc::downgrade(&q2),
            head: Mutex::new(None),
        };
        let c2 = Channel {
            peer_id,
            me: q2,
            peer: Arc::downgrade(&q1),
            head: Mutex::new(None),
        };
        (c1, c2)
    }

    #[inline]
    pub fn peer_eq(&self, other: &Self) -> bool {
        self.peer_id == other.peer_id
    }

    #[inline]
    pub fn event(&self) -> &Arc<BasicEvent> {
        &self.me.event
    }

    /// # Errors
    ///
    /// Returns error if the peer is closed or if the channel is full.
    pub fn send(&self, msg: &mut Packet) -> sv_call::Result {
        let peer = self.peer.upgrade().ok_or(sv_call::EPIPE)?;
        if peer.msgs.len() >= MAX_QUEUE_SIZE {
            Err(sv_call::ENOSPC)
        } else {
            peer.msgs.push(mem::take(msg));
            peer.event.notify(0, SIG_READ);
            Ok(())
        }
    }

    /// # Safety
    ///
    /// `head` must contains a valid packet.
    unsafe fn get_packet(
        head: &mut Option<Packet>,
        buffer_cap: &mut usize,
        handle_cap: &mut usize,
    ) -> sv_call::Result<Packet> {
        let packet = unsafe { head.as_mut().unwrap_unchecked() };
        let buffer_size = packet.buffer().len();
        let handle_count = packet.object_count();
        let ret = if buffer_size > *buffer_cap || handle_count > *handle_cap {
            Err(sv_call::EBUFFER)
        } else {
            Ok(unsafe { head.take().unwrap_unchecked() })
        };
        *buffer_cap = buffer_size;
        *handle_cap = handle_count;
        ret
    }

    /// # Errors
    ///
    /// Returns error if the peer is closed.
    pub fn receive(
        &self,
        buffer_cap: &mut usize,
        handle_cap: &mut usize,
    ) -> sv_call::Result<Packet> {
        let _pree = PREEMPT.lock();
        let mut head = self.head.lock();
        if head.is_none() {
            let err = if self.peer.strong_count() > 0 {
                sv_call::ENOENT
            } else {
                sv_call::EPIPE
            };
            *head = Some(self.me.msgs.pop().ok_or(err)?);
        }
        unsafe { Self::get_packet(&mut head, buffer_cap, handle_cap) }
    }
}

unsafe impl DefaultFeature for Channel {
    fn default_features() -> Feature {
        Feature::SEND | Feature::READ | Feature::WRITE | Feature::WAIT
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        if let Some(peer) = self.peer.upgrade() {
            peer.event.cancel();
        }
    }
}
