use alloc::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};
use core::{mem, time::Duration};

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
    msgs: WaitQueue<Packet>,
    callers: Mutex<BTreeMap<usize, Arc<WaitCell<Packet>>>>,
}

impl Default for ChannelSide {
    #[inline]
    fn default() -> Self {
        ChannelSide {
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

    pub fn is_peer(&self, other: &Self) -> bool {
        self.peer_eq(other)
    }

    #[inline]
    pub fn peer_eq(&self, other: &Self) -> bool {
        self.peer_id == other.peer_id
    }

    /// # Errors
    ///
    /// Returns error if the peer is closed or if the channel is full.
    pub fn send(&self, msg: &mut Packet) -> solvent::Result {
        match self.peer.upgrade() {
            None => Err(solvent::Error::EPIPE),
            Some(peer) => {
                let called = PREEMPT.scope(|| {
                    let mut callers = peer.callers.lock();
                    callers.remove(&msg.id)
                });
                match called {
                    Some(caller) => {
                        caller.replace(mem::take(msg));
                        Ok(())
                    }
                    None => {
                        if peer.msgs.len() >= MAX_QUEUE_SIZE {
                            Err(solvent::Error::ENOSPC)
                        } else {
                            peer.msgs.push(mem::take(msg));
                            Ok(())
                        }
                    }
                }
            }
        }
    }

    /// # Errors
    ///
    /// Returns error if the peer is closed.
    pub fn receive(
        &self,
        timeout: Duration,
        buffer_cap: usize,
        handle_cap: usize,
    ) -> solvent::Result<Packet> {
        let _pree = PREEMPT.lock();
        let mut head = self.head.lock();
        if head.is_none() {
            *head = Some(if timeout.is_zero() {
                self.me.msgs.try_pop().ok_or(solvent::Error::ENOENT)?
            } else {
                self.me
                    .msgs
                    .pop(timeout, "Channel::receive")
                    .ok_or(solvent::Error::EPIPE)?
            });
        }
        let packet = unsafe { head.as_mut().unwrap_unchecked() };
        if packet.buffer().len() > buffer_cap || packet.object_count() > handle_cap {
            Err(solvent::Error::EBUFFER)
        } else {
            Ok(unsafe { head.take().unwrap_unchecked() })
        }
    }
}

mod syscall {
    use core::slice;

    use solvent::{ipc::RawPacket, *};

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

    #[syscall]
    fn chan_send(hdl: Handle, packet: UserPtr<In, RawPacket>) -> Result {
        hdl.check_null()?;

        let packet = unsafe { packet.read()? };
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
            channel.send(&mut packet).map_err(Into::into)
        })
    }

    #[syscall]
    fn chan_recv(hdl: Handle, packet_ptr: UserPtr<InOut, RawPacket>, timeout_us: u64) -> Result {
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
            channel
                .receive(timeout, user_buffer.len(), user_handles.len())
                .map(|mut packet| {
                    map.receive(&mut packet.objects, user_handles);
                    packet
                })
        })?;

        user_packet.id = packet.id;
        let data = packet.buffer();
        user_buffer[..data.len()].copy_from_slice(data);

        unsafe {
            packet_ptr.out().write(user_packet).unwrap();
        }

        Ok(())
    }
}
