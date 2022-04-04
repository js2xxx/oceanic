mod syscall;

use alloc::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};
use core::{
    mem,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
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

#[derive(Debug, Default)]
struct Caller {
    cell: Option<Packet>,
    event: Arc<BasicEvent>,
    head: Option<Packet>,
}

#[derive(Debug)]
struct ChannelSide {
    msg_id: AtomicUsize,
    msgs: SegQueue<Packet>,
    event: Arc<BasicEvent>,
    callers: Mutex<BTreeMap<usize, Caller>>,
}

impl Default for ChannelSide {
    #[inline]
    fn default() -> Self {
        ChannelSide {
            msg_id: AtomicUsize::new(sv_call::ipc::CUSTOM_MSG_ID_END),
            msgs: SegQueue::new(),
            event: BasicEvent::new(0),
            callers: Mutex::new(BTreeMap::new()),
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
        // TODO: Find a better way to acquire an unique id.
        let peer_id = unsafe { archop::msr::rdtsc() };
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
        match self.peer.upgrade() {
            None => Err(sv_call::Error::EPIPE),
            Some(peer) => {
                let called = PREEMPT.scope(|| {
                    let mut callers = peer.callers.lock();
                    let called = callers.get_mut(&msg.id);
                    if let Some(caller) = called {
                        let _old = caller.cell.replace(mem::take(msg));
                        caller.event.notify(0, SIG_READ);
                        debug_assert!(_old.is_none());
                        true
                    } else {
                        false
                    }
                });
                if called {
                    Ok(())
                } else if peer.msgs.len() >= MAX_QUEUE_SIZE {
                    Err(sv_call::Error::ENOSPC)
                } else {
                    peer.msgs.push(mem::take(msg));
                    peer.event.notify(0, SIG_READ);
                    Ok(())
                }
            }
        }
    }

    fn get_packet(
        head: &mut Option<Packet>,
        buffer_cap: &mut usize,
        handle_cap: &mut usize,
    ) -> sv_call::Result<Packet> {
        let packet = unsafe { head.as_mut().unwrap_unchecked() };
        let buffer_size = packet.buffer().len();
        let handle_count = packet.object_count();
        let ret = if buffer_size > *buffer_cap || handle_count > *handle_cap {
            Err(sv_call::Error::EBUFFER)
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
        if self.peer.strong_count() == 0 {
            return Err(sv_call::Error::EPIPE);
        }
        let _pree = PREEMPT.lock();
        let mut head = self.head.lock();
        if head.is_none() {
            *head = Some(self.me.msgs.pop().ok_or(sv_call::Error::ENOENT)?);
        }
        Self::get_packet(&mut head, buffer_cap, handle_cap)
    }

    #[inline]
    fn next_msg_id(id: &AtomicUsize) -> usize {
        id.fetch_update(SeqCst, SeqCst, |id| {
            Some(if id == usize::MAX {
                sv_call::ipc::CUSTOM_MSG_ID_END
            } else {
                id + 1
            })
        })
        .unwrap()
    }

    pub fn call_send(&self, msg: &mut Packet) -> sv_call::Result<usize> {
        match self.peer.upgrade() {
            None => Err(sv_call::Error::EPIPE),
            Some(peer) => {
                if peer.msgs.len() >= MAX_QUEUE_SIZE {
                    Err(sv_call::Error::ENOSPC)
                } else {
                    let id = Self::next_msg_id(&self.me.msg_id);
                    msg.id = id;
                    self.me
                        .callers
                        .lock()
                        .try_insert(id, Caller::default())
                        .map_err(|_| sv_call::Error::EEXIST)?;
                    peer.msgs.push(mem::take(msg));
                    peer.event.notify(0, SIG_READ);
                    Ok(id)
                }
            }
        }
    }

    fn call_event(&self, id: usize) -> sv_call::Result<Arc<BasicEvent>> {
        PREEMPT.scope(|| {
            let callers = self.me.callers.lock();
            callers.get(&id).map_or(Err(sv_call::Error::ENOENT), |ent| {
                Ok(Arc::clone(&ent.event))
            })
        })
    }

    pub fn call_receive(
        &self,
        id: usize,
        buffer_cap: &mut usize,
        handle_cap: &mut usize,
    ) -> sv_call::Result<Packet> {
        if self.peer.strong_count() == 0 {
            return Err(sv_call::Error::EPIPE);
        }
        let _pree = PREEMPT.lock();
        let mut callers = self.me.callers.lock();
        let mut caller = match callers.entry(id) {
            alloc::collections::btree_map::Entry::Vacant(_) => return Err(sv_call::Error::ENOENT),
            alloc::collections::btree_map::Entry::Occupied(caller) => caller,
        };
        if caller.get().head.is_none() {
            let packet = caller.get_mut().cell.take().ok_or(sv_call::Error::ENOENT)?;
            caller.get_mut().head = Some(packet);
        }
        Self::get_packet(&mut caller.get_mut().head, buffer_cap, handle_cap)
            .inspect(|_| drop(caller.remove()))
    }
}

unsafe impl DefaultFeature for Channel {
    fn default_features() -> Feature {
        Feature::SEND | Feature::READ | Feature::WRITE | Feature::WAIT
    }
}
