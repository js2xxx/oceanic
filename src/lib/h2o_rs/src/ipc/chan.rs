#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::{mem::MaybeUninit, time::Duration};

use sv_call::{c_ty::Status, ipc::RawPacket, Syscall};

use super::Dispatcher;
#[cfg(feature = "alloc")]
use super::{Packet, PacketTyped};
use crate::{error::*, obj::Object, prelude::Disp2};

#[repr(transparent)]
pub struct Channel(sv_call::Handle);

crate::impl_obj!(Channel);
crate::impl_obj!(@DROP, Channel);

impl Channel {
    pub fn try_new() -> Result<(Channel, Channel)> {
        let (mut h1, mut h2) = (sv_call::Handle::NULL, sv_call::Handle::NULL);
        unsafe { sv_call::sv_chan_new(&mut h1, &mut h2).into_res()? };

        // SAFETY: The handles are freshly allocated.
        Ok(unsafe { (Channel::from_raw(h1), Channel::from_raw(h2)) })
    }

    pub fn new() -> (Channel, Channel) {
        Self::try_new().expect("Failed to create a pair of channels")
    }

    pub fn send_raw(
        &self,
        id: Option<usize>,
        buffer: &[u8],
        handles: &[sv_call::Handle],
    ) -> Result {
        let packet = RawPacket {
            id: id.unwrap_or_default(),
            handles: handles.as_ptr() as *mut _,
            handle_count: handles.len(),
            handle_cap: handles.len(),
            buffer: buffer.as_ptr() as *mut _,
            buffer_size: buffer.len(),
            buffer_cap: buffer.len(),
        };
        // SAFETY: We don't move the ownership of the handle.
        unsafe { sv_call::sv_chan_send(unsafe { self.raw() }, &packet).into_res() }
    }

    #[cfg(feature = "alloc")]
    pub fn send_packet(&self, packet: &mut Packet) -> Result {
        self.send_raw(packet.id, &packet.buffer, &packet.handles)
            .map(|_| *packet = Default::default())
    }

    #[cfg(feature = "alloc")]
    pub fn send<T: PacketTyped>(&self, packet: T) -> Result {
        self.send_packet(&mut packet.into_packet())
    }

    pub fn receive_raw(
        &self,
        buffer: &mut [u8],
        handles: &mut [MaybeUninit<sv_call::Handle>],
    ) -> (Result<usize>, usize, usize) {
        let mut packet = RawPacket {
            id: 0,
            handles: handles.as_mut_ptr().cast(),
            handle_count: handles.len(),
            handle_cap: handles.len(),
            buffer: buffer.as_mut_ptr(),
            buffer_size: buffer.len(),
            buffer_cap: buffer.len(),
        };
        // SAFETY: We don't move the ownership of the handle.
        let res = unsafe { sv_call::sv_chan_recv(unsafe { self.raw() }, &mut packet).into_res() };
        (
            res.map(|_| packet.id),
            packet.buffer_size,
            packet.handle_count,
        )
    }

    pub fn pack_receive(&self, packet: &mut Packet) -> PackRecv {
        let buffer = &mut packet.buffer;
        let handles = packet.handles.spare_capacity_mut();
        let mut packet = RawPacket {
            id: 0,
            handles: handles.as_mut_ptr().cast(),
            handle_count: handles.len(),
            handle_cap: handles.len(),
            buffer: buffer.as_mut_ptr(),
            buffer_size: buffer.len(),
            buffer_cap: buffer.len(),
        };
        let syscall = unsafe { sv_call::sv_pack_chan_recv(unsafe { self.raw() }, &mut packet) };
        PackRecv {
            raw_packet: packet,
            syscall,
        }
    }

    #[cfg(feature = "alloc")]
    pub fn receive_into(
        &self,
        buffer: &mut Vec<u8>,
        handles: &mut Vec<sv_call::Handle>,
    ) -> Result<usize> {
        receive_into_impl(|buf, hdl| self.receive_raw(buf, hdl), buffer, handles)
    }

    #[cfg(feature = "alloc")]
    pub fn receive_packet(&self, packet: &mut Packet) -> Result {
        let id = self.receive_into(&mut packet.buffer, &mut packet.handles)?;
        packet.id = Some(id);
        Ok(())
    }

    #[cfg(feature = "alloc")]
    pub fn try_receive<T: PacketTyped>(
        &self,
    ) -> Result<core::result::Result<T, (T::TryFromError, Packet)>> {
        let mut packet = Default::default();
        self.receive_packet(&mut packet)?;
        match T::try_from_packet(&mut packet) {
            Ok(packet) => Ok(Ok(packet)),
            Err(err) => Ok(Err((err, packet))),
        }
    }

    /// Warning: If the type of the received packet is not the requested type,
    /// then the packet will be discarded!
    #[cfg(feature = "alloc")]
    pub fn receive<T: PacketTyped>(&self) -> Result<T> {
        let mut packet = Default::default();
        self.receive_packet(&mut packet)?;
        T::try_from_packet(&mut packet).map_err(Into::into)
    }

    pub fn call_send_raw(&self, buffer: &[u8], handles: &[sv_call::Handle]) -> Result<usize> {
        let packet = RawPacket {
            id: 0,
            handles: handles.as_ptr() as *mut _,
            handle_count: handles.len(),
            handle_cap: handles.len(),
            buffer: buffer.as_ptr() as *mut _,
            buffer_size: buffer.len(),
            buffer_cap: buffer.len(),
        };

        unsafe {
            // SAFETY: We don't move the ownership of the handle.
            sv_call::sv_chan_csend(unsafe { self.raw() }, &packet)
                .into_res()
                .map(|value| value as usize)
        }
    }

    #[cfg(feature = "alloc")]
    pub fn call_send(&self, packet: &Packet) -> Result<usize> {
        self.call_send_raw(&packet.buffer, &packet.handles)
    }

    pub fn call_receive_raw(
        &self,
        id: usize,
        buffer: &mut [u8],
        handles: &mut [MaybeUninit<sv_call::Handle>],
        timeout: Duration,
    ) -> (Result, usize, usize) {
        let mut packet = RawPacket {
            id: 0,
            handles: handles.as_mut_ptr().cast(),
            handle_count: handles.len(),
            handle_cap: handles.len(),
            buffer: buffer.as_mut_ptr(),
            buffer_size: buffer.len(),
            buffer_cap: buffer.len(),
        };
        let timeout_us = match crate::time::try_into_us(timeout) {
            Ok(us) => us,
            Err(err) => return (Err(err), 0, 0),
        };
        // SAFETY: We don't move the ownership of the handle.
        let res = unsafe {
            sv_call::sv_chan_crecv(unsafe { self.raw() }, id, &mut packet, timeout_us).into_res()
        };
        (res, packet.buffer_size, packet.handle_count)
    }

    pub fn pack_call_receive(&self, id: usize, packet: &mut Packet) -> PackRecv {
        let buffer = &mut packet.buffer;
        let handles = packet.handles.spare_capacity_mut();
        let mut packet = RawPacket {
            id: 0,
            handles: handles.as_mut_ptr().cast(),
            handle_count: handles.len(),
            handle_cap: handles.len(),
            buffer: buffer.as_mut_ptr(),
            buffer_size: buffer.len(),
            buffer_cap: buffer.len(),
        };
        let syscall =
            unsafe { sv_call::sv_pack_chan_crecv(unsafe { self.raw() }, id, &mut packet, 0) };
        PackRecv {
            raw_packet: packet,
            syscall,
        }
    }

    #[cfg(feature = "alloc")]
    pub fn call_receive_into(
        &self,
        id: usize,
        buffer: &mut Vec<u8>,
        handles: &mut Vec<sv_call::Handle>,
        timeout: Duration,
    ) -> Result {
        receive_into_impl(
            |buf, hdl| self.call_receive_raw(id, buf, hdl, timeout),
            buffer,
            handles,
        )
    }

    #[cfg(feature = "alloc")]
    pub fn call_receive(&self, id: usize, packet: &mut Packet, timeout: Duration) -> Result {
        self.call_receive_into(id, &mut packet.buffer, &mut packet.handles, timeout)
    }

    pub fn call_receive_async(&self, id: usize, wake_all: bool) -> Result<super::Blocker> {
        // SAFETY: We don't move the ownership of the handle.
        let handle =
            unsafe { sv_call::sv_chan_acrecv(unsafe { self.raw() }, id, wake_all).into_res()? };
        // SAFETY: The handle is freshly allocated.
        Ok(unsafe { super::Blocker::from_raw(handle) })
    }

    pub fn call_receive_async2(&self, id: usize, disp: &Dispatcher) -> Result<usize> {
        let key =
            unsafe { sv_call::sv_chan_acrecv2(unsafe { self.raw() }, id, unsafe { disp.raw() }) }
                .into_res()?;
        Ok(key as usize)
    }

    pub fn call_receive_async3(&self, id: usize, disp: &Disp2, syscall: &Syscall) -> Result<usize> {
        let key = unsafe {
            sv_call::sv_chan_acrecv3(unsafe { self.raw() }, id, unsafe { disp.raw() }, syscall)
                .into_res()
        }?;
        Ok(key as usize)
    }

    #[cfg(feature = "alloc")]
    pub fn call(&self, packet: &mut Packet, timeout: Duration) -> Result {
        let id = self.call_send(packet)?;
        self.call_receive(id, packet, timeout)
    }

    #[cfg(feature = "alloc")]
    pub fn handle<F, R>(&self, handler: F) -> Result<R>
    where
        F: FnOnce(&mut Packet) -> Result<R>,
    {
        let mut packet = Packet::default();
        self.receive_packet(&mut packet)?;
        let id = packet.id;
        let ret = handler(&mut packet)?;
        packet.id = id;
        self.send_packet(&mut packet)?;
        Ok(ret)
    }
}

#[cfg(feature = "alloc")]
fn receive_into_impl<F, R>(
    mut receiver: F,
    buffer: &mut Vec<u8>,
    handles: &mut Vec<sv_call::Handle>,
) -> Result<R>
where
    F: FnMut(&mut [u8], &mut [MaybeUninit<sv_call::Handle>]) -> (Result<R>, usize, usize),
{
    handles.clear();

    // We use smaller stack-based buffers to avoid dangling pointers in empty
    // vectors and to reduce times of heap allocations.
    let mut min_buffer = [0u8; 8];
    let mut min_handles = [MaybeUninit::uninit(); 4];
    match receiver(&mut min_buffer, &mut min_handles) {
        (Ok(value), buffer_size, handle_count) => {
            buffer.resize(buffer_size, 0);
            buffer.copy_from_slice(&min_buffer[..buffer_size]);

            handles.reserve(handle_count);
            handles
                .spare_capacity_mut()
                .copy_from_slice(&min_handles[..handle_count]);
            // SAFETY: `handles` is ensured to have the given numbers of elements.
            unsafe { handles.set_len(handle_count) };
            return Ok(value);
        }
        (Err(EBUFFER), buffer_size, handle_count) => {
            buffer.reserve(buffer_size);
            handles.reserve(handle_count);
        }
        (Err(err), ..) => return Err(err),
    }

    loop {
        let buffer_cap = buffer.capacity();
        let handle_cap = handles.capacity();

        // SAFETY: u8 doesn't implement `Drop` so we always consider it valid.
        unsafe { buffer.set_len(buffer.capacity()) };
        match receiver(&mut *buffer, handles.spare_capacity_mut()) {
            (Ok(value), buffer_size, handle_count) => {
                // SAFETY: `buffer` and `handles` are ensured to have the given numbers of
                // elements.
                unsafe {
                    buffer.set_len(buffer_size);
                    handles.set_len(handle_count);
                }
                break Ok(value);
            }
            (Err(EBUFFER), buffer_size, handle_count) => {
                buffer.reserve(buffer_size.saturating_sub(buffer_cap));
                handles.reserve(handle_count.saturating_sub(handle_cap));
            }
            (Err(err), ..) => break Err(err),
        }
    }
}

pub struct PackRecv {
    pub raw_packet: RawPacket,
    pub syscall: Syscall,
}

impl PackRecv {
    pub fn receive(&self, res: Status) -> (Result<usize>, usize, usize) {
        let res = res.into_res();
        (
            res.map(|_| self.raw_packet.id),
            self.raw_packet.buffer_size,
            self.raw_packet.handle_count,
        )
    }
}
