use alloc::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};
use core::{
    mem,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
    time::Duration,
};

use bytes::Bytes;
use spin::Mutex;

use crate::sched::{
    task::hdl,
    wait::{WaitCell, WaitQueue},
    PREEMPT, SCHED,
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
    cell: WaitCell<Packet>,
    head: Option<Packet>,
}

#[derive(Debug)]
struct ChannelSide {
    msg_id: AtomicUsize,
    msgs: WaitQueue<Packet>,
    callers: Mutex<BTreeMap<usize, Caller>>,
}

impl Default for ChannelSide {
    #[inline]
    fn default() -> Self {
        ChannelSide {
            msg_id: AtomicUsize::new(sv_call::ipc::CUSTOM_MSG_ID_END),
            msgs: WaitQueue::new(),
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

    /// # Errors
    ///
    /// Returns error if the peer is closed or if the channel is full.
    pub fn send(&self, msg: &mut Packet) -> sv_call::Result {
        match self.peer.upgrade() {
            None => Err(sv_call::Error::EPIPE),
            Some(peer) => {
                let called = PREEMPT.scope(|| {
                    let callers = peer.callers.lock();
                    let called = callers.get(&msg.id);
                    if let Some(caller) = called {
                        let _old = caller.cell.replace(mem::take(msg));
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
        timeout: Duration,
        buffer_cap: &mut usize,
        handle_cap: &mut usize,
    ) -> sv_call::Result<Packet> {
        let _pree = PREEMPT.lock();
        let mut head = self.head.lock();
        if head.is_none() {
            *head = Some(if timeout.is_zero() {
                self.me.msgs.try_pop().ok_or(sv_call::Error::ENOENT)?
            } else {
                self.me
                    .msgs
                    .pop(timeout, "Channel::receive")
                    .ok_or(sv_call::Error::EPIPE)?
            });
        }
        Self::get_packet(&mut head, buffer_cap, handle_cap)
    }

    pub fn call_send(&self, msg: &mut Packet) -> sv_call::Result<usize> {
        match self.peer.upgrade() {
            None => Err(sv_call::Error::EPIPE),
            Some(peer) => {
                if peer.msgs.len() >= MAX_QUEUE_SIZE {
                    Err(sv_call::Error::ENOSPC)
                } else {
                    let id = self
                        .me
                        .msg_id
                        .fetch_update(SeqCst, SeqCst, |id| {
                            Some(if id == usize::MAX {
                                sv_call::ipc::CUSTOM_MSG_ID_END
                            } else {
                                id + 1
                            })
                        })
                        .unwrap();
                    msg.id = id;
                    self.me
                        .callers
                        .lock()
                        .try_insert(id, Caller::default())
                        .map_err(|_| sv_call::Error::EEXIST)?;
                    peer.msgs.push(mem::take(msg));
                    Ok(id)
                }
            }
        }
    }

    pub fn call_receive(
        &self,
        id: usize,
        timeout: Duration,
        buffer_cap: &mut usize,
        handle_cap: &mut usize,
    ) -> sv_call::Result<Packet> {
        let _pree = PREEMPT.lock();
        let mut callers = self.me.callers.lock();
        let mut caller = match callers.entry(id) {
            alloc::collections::btree_map::Entry::Vacant(_) => return Err(sv_call::Error::ENOENT),
            alloc::collections::btree_map::Entry::Occupied(caller) => caller,
        };
        if caller.get().head.is_none() {
            let packet = if timeout.is_zero() {
                caller.get().cell.try_take().ok_or(sv_call::Error::ENOENT)?
            } else {
                caller
                    .get()
                    .cell
                    .take(timeout, "Channel::call_receive")
                    .ok_or(sv_call::Error::EPIPE)?
            };
            caller.get_mut().head = Some(packet);
        }
        Self::get_packet(&mut caller.get_mut().head, buffer_cap, handle_cap)
            .inspect(|_| drop(caller.remove()))
    }
}

mod syscall {
    use core::slice;

    use sv_call::{
        ipc::{RawPacket, MAX_BUFFER_SIZE, MAX_HANDLE_COUNT},
        *,
    };

    use super::*;
    use crate::syscall::{In, InOut, Out, UserPtr};

    #[syscall]
    fn chan_new(p1: UserPtr<Out, Handle>, p2: UserPtr<Out, Handle>) -> Result {
        p1.check()?;
        p2.check()?;
        SCHED.with_current(|cur| {
            let (c1, c2) = Channel::new();
            let map = cur.tid().handles();
            unsafe {
                let h1 = map.insert_unchecked(c1, true, false)?;
                let h2 = map.insert_unchecked(c2, true, false)?;
                p1.write(h1)?;
                p2.write(h2)
            }
        })
    }

    fn chan_send_impl<F, R>(hdl: Handle, packet: UserPtr<In, RawPacket>, send: F) -> Result<R>
    where
        F: FnOnce(&Channel, &mut Packet) -> Result<R>,
    {
        hdl.check_null()?;

        let packet = unsafe { packet.read()? };
        if packet.buffer_size > MAX_BUFFER_SIZE || packet.handle_count >= MAX_HANDLE_COUNT {
            return Err(Error::ENOMEM);
        }
        UserPtr::<In, Handle>::new(packet.handles).check_slice(packet.handle_count)?;
        UserPtr::<In, u8>::new(packet.buffer).check_slice(packet.buffer_size)?;

        let handles = unsafe { slice::from_raw_parts(packet.handles, packet.handle_count) };
        if handles.contains(&hdl) {
            return Err(Error::EPERM);
        }
        let buffer = unsafe { slice::from_raw_parts(packet.buffer, packet.buffer_size) };

        SCHED.with_current(|cur| {
            let map = cur.tid().handles();
            let channel = map.get::<Channel>(hdl)?;
            let objects = unsafe { map.send(handles, channel) }?;
            let mut packet = Packet::new(packet.id, objects, buffer);
            send(channel, &mut packet)
        })
    }

    fn chan_recv_impl<F>(
        hdl: Handle,
        packet_ptr: UserPtr<InOut, RawPacket>,
        timeout_us: u64,
        recv: F,
    ) -> Result
    where
        F: FnOnce(&Channel, Duration, usize, usize) -> (Result<Packet>, usize, usize),
    {
        hdl.check_null()?;
        let timeout = if timeout_us == u64::MAX {
            Duration::MAX
        } else {
            Duration::from_micros(timeout_us)
        };

        let mut user_packet = unsafe { packet_ptr.r#in().read()? };
        UserPtr::<Out, Handle>::new(user_packet.handles).check_slice(user_packet.handle_cap)?;
        UserPtr::<Out, u8>::new(user_packet.buffer).check_slice(user_packet.buffer_cap)?;

        let user_handles =
            unsafe { slice::from_raw_parts_mut(user_packet.handles, user_packet.handle_cap) };
        let user_buffer =
            unsafe { slice::from_raw_parts_mut(user_packet.buffer, user_packet.buffer_cap) };

        let packet = SCHED.with_current(|cur| {
            let map = cur.tid().handles();

            let channel = map.get::<Channel>(hdl)?;
            let (res, buffer_size, handle_count) =
                recv(channel, timeout, user_buffer.len(), user_handles.len());
            user_packet.buffer_size = buffer_size;
            user_packet.handle_count = handle_count;
            res.map(|mut packet| {
                map.receive(&mut packet.objects, user_handles);
                packet
            })
        })?;

        user_packet.id = packet.id;
        let data = packet.buffer();
        user_buffer[..data.len()].copy_from_slice(data);

        unsafe { packet_ptr.out().write(user_packet)? };

        Ok(())
    }

    #[syscall]
    fn chan_send(hdl: Handle, packet: UserPtr<In, RawPacket>) -> Result {
        chan_send_impl(hdl, packet, |channel, packet| channel.send(packet))
    }

    #[syscall]
    fn chan_recv(hdl: Handle, packet_ptr: UserPtr<InOut, RawPacket>, timeout_us: u64) -> Result {
        chan_recv_impl(
            hdl,
            packet_ptr,
            timeout_us,
            |channel, timeout, mut buffer_cap, mut handle_cap| {
                let ret = channel.receive(timeout, &mut buffer_cap, &mut handle_cap);
                (ret, buffer_cap, handle_cap)
            },
        )
    }

    #[syscall]
    fn chan_csend(hdl: Handle, packet: UserPtr<In, RawPacket>) -> Result<usize> {
        chan_send_impl(hdl, packet, |channel, packet| channel.call_send(packet))
    }

    #[syscall]
    fn chan_crecv(
        hdl: Handle,
        id: usize,
        packet_ptr: UserPtr<InOut, RawPacket>,
        timeout_us: u64,
    ) -> Result {
        chan_recv_impl(
            hdl,
            packet_ptr,
            timeout_us,
            |channel, timeout, mut buffer_cap, mut handle_cap| {
                let ret = channel.call_receive(id, timeout, &mut buffer_cap, &mut handle_cap);
                (ret, buffer_cap, handle_cap)
            },
        )
    }
}
