use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use core::time::Duration;

use bytes::Bytes;
use solvent::Handle;
use spin::{Mutex, MutexGuard};

use super::{IpcError, Object};
use crate::sched::{task, wait::WaitQueue, SCHED};

const MAX_QUEUE_SIZE: usize = 2048;

#[derive(Debug)]
pub struct Packet {
    objects: Vec<Object>,
    buffer: Bytes,
}

impl Packet {
    pub fn new(objects: Vec<Object>, data: &[u8]) -> Self {
        let buffer = Bytes::copy_from_slice(data);
        Packet { objects, buffer }
    }

    #[inline]
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    #[inline]
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    #[inline]
    pub unsafe fn process(self, cur: &mut task::Ready) -> Vec<Handle> {
        cur.tid().handles().write().receive(self.objects)
    }
}

pub struct Channel {
    peer_id: u64,
    me: Arc<WaitQueue<Packet>>,
    peer: Weak<WaitQueue<Packet>>,
    head: Mutex<Option<Packet>>,
}

impl Channel {
    pub fn new() -> (Self, Self) {
        // TODO: Find a better way to acquire an unique id.
        let peer_id = unsafe { archop::msr::rdtsc() };
        let q1 = Arc::new(WaitQueue::new());
        let q2 = Arc::new(WaitQueue::new());
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
        self.peer_id == other.peer_id
    }

    pub fn send(&self, msg: Packet) -> Result<(), IpcError> {
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

    pub fn receive<'a>(
        &'a self,
        timeout: Duration,
    ) -> Result<MutexGuard<'a, Option<Packet>>, IpcError> {
        let mut head = self.head.lock();
        if head.is_none() {
            *head = Some(
                self.me
                    .pop(timeout, "Channel::receive")
                    .ok_or(IpcError::QueueEmpty)?,
            );
        }
        Ok(head)
    }

    pub fn try_receive<'a>(&'a self) -> Result<MutexGuard<'a, Option<Packet>>, IpcError> {
        let mut head = self.head.lock();
        if head.is_none() {
            *head = Some(self.me.try_pop().ok_or(IpcError::QueueEmpty)?);
        }
        Ok(head)
    }
}

mod syscall {
    use core::slice;

    use solvent::{ipc::RawPacket, *};

    use super::*;
    use crate::syscall::{In, InOut, Out, UserPtr};

    #[syscall]
    fn chan_new(p1: UserPtr<Out, Handle>, p2: UserPtr<Out, Handle>) {
        p1.check()?;
        p2.check()?;
        let ret = SCHED.with_current(|cur| {
            let (c1, c2) = Channel::new();
            let mut map = cur.tid().handles().write();
            let h1 = map.insert(c1);
            let h2 = map.insert(c2);
            unsafe {
                p1.write(h1).unwrap();
                p2.write(h2).unwrap();
            }
        });
        ret.ok_or(Error(ESRCH))
    }

    #[syscall]
    fn chan_send(hdl: Handle, packet: UserPtr<In, RawPacket>) {
        hdl.check_null()?;

        let packet = unsafe { packet.read()? };
        UserPtr::<In, Handle>::new(packet.handles).check_slice(packet.handle_count)?;
        UserPtr::<In, u8>::new(packet.buffer).check_slice(packet.buffer_size)?;

        let handles = unsafe { slice::from_raw_parts(packet.handles, packet.handle_count) };
        if handles.contains(&hdl) {
            return Err(Error(EPERM));
        }
        let buffer = unsafe { slice::from_raw_parts(packet.buffer, packet.buffer_size) };

        let ret = SCHED.with_current(|cur| {
            let mut map = cur.tid().handles().write();

            let (objects, channel) = unsafe { map.send_for_channel(handles, hdl) }?;
            let packet = Packet::new(objects, buffer);
            channel.send(packet).map_err(Into::into)
        });
        ret.ok_or(Error(ESRCH)).flatten()
    }

    #[syscall]
    fn chan_recv(hdl: Handle, packet_ptr: UserPtr<InOut, RawPacket>, timeout_us: u64) {
        hdl.check_null()?;
        let timeout = if timeout_us == u64::MAX {
            Duration::MAX
        } else {
            Duration::from_micros(timeout_us)
        };

        let mut user_packet = unsafe { packet_ptr.r#in().read()? };
        UserPtr::<In, Handle>::new(user_packet.handles).check_slice(user_packet.handle_cap)?;
        UserPtr::<In, u8>::new(user_packet.buffer).check_slice(user_packet.buffer_cap)?;

        let user_handles =
            unsafe { slice::from_raw_parts_mut(user_packet.handles, user_packet.handle_cap) };
        let user_buffer =
            unsafe { slice::from_raw_parts_mut(user_packet.buffer, user_packet.buffer_cap) };

        let ret = SCHED.with_current(|cur| {
            let mut map = cur.tid().handles().write();

            let packet = {
                let channel = map.get::<Channel>(hdl).ok_or(Error(EINVAL))?;

                let mut packet = if !timeout.is_zero() {
                    channel.receive(timeout)
                } else {
                    channel.try_receive()
                }
                .map_err(Into::into)?;

                let data = packet.as_ref().unwrap().buffer();
                let object_count = packet.as_ref().unwrap().object_count();

                {
                    let mut ebuf = false;
                    if data.len() > user_buffer.len() {
                        user_packet.buffer_size = data.len();
                        ebuf = true;
                    }
                    if object_count > user_handles.len() {
                        user_packet.handle_count = object_count;
                        ebuf = true;
                    }
                    if ebuf {
                        return Err(Error(EBUFFER));
                    }
                }

                user_buffer[..data.len()].copy_from_slice(data);
                packet.take()
            };

            let handles = packet.map(|p| map.receive(p.objects)).unwrap();
            user_handles[..handles.len()].copy_from_slice(&handles);

            unsafe {
                drop(user_buffer);
                drop(user_handles);
                packet_ptr.out().write(user_packet).unwrap();
            }

            Ok(())
        });
        let u = ret.ok_or(Error(ESRCH));
        u.flatten()
    }
}
