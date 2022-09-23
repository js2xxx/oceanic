#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
use core::mem::MaybeUninit;

use sv_call::{c_ty::Status, ipc::RawPacket, Syscall};

#[cfg(feature = "alloc")]
use super::{Packet, PacketTyped};
use crate::{error::*, obj::Object};

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

    #[cfg(feature = "alloc")]
    pub fn pack_receive(&self, mut packet: Packet) -> PackRecv {
        let buffer = &mut packet.buffer;
        let handles = packet.handles.spare_capacity_mut();
        let mut raw_packet = Box::new(RawPacket {
            id: 0,
            handles: handles.as_mut_ptr().cast(),
            handle_count: handles.len(),
            handle_cap: handles.len(),
            buffer: buffer.as_mut_ptr(),
            buffer_size: buffer.len(),
            buffer_cap: buffer.len(),
        });
        let syscall =
            unsafe { sv_call::sv_pack_chan_recv(unsafe { self.raw() }, &mut *raw_packet) };
        PackRecv {
            packet,
            raw_packet,
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
    buffer.clear();
    handles.clear();

    // We use smaller stack-based buffers to avoid dangling pointers in empty
    // vectors and to reduce times of heap allocations.
    let mut min_buffer = [0u8; 8];
    let mut min_handles = [MaybeUninit::uninit(); 4];
    match receiver(&mut min_buffer, &mut min_handles) {
        (Ok(value), buffer_size, handle_count) => {
            buffer.resize(buffer_size, 0);
            if buffer_size > 0 {
                buffer.copy_from_slice(&min_buffer[..buffer_size]);
            }

            if handle_count > 0 {
                handles.reserve(handle_count - handles.capacity());
                handles
                    .spare_capacity_mut()
                    .copy_from_slice(&min_handles[..handle_count]);
            }
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

#[cfg(feature = "alloc")]
pub struct PackRecv {
    pub packet: Packet,
    pub raw_packet: Box<RawPacket>,
    pub syscall: Syscall,
}

#[cfg(feature = "alloc")]
unsafe impl Send for PackRecv {}

#[cfg(feature = "alloc")]
impl PackRecv {
    pub fn receive(&self, res: Status, canceled: bool) -> (Result<usize>, usize, usize) {
        let res = res.into_res().and((!canceled).then_some(()).ok_or(ETIME));
        (
            res.map(|_| self.raw_packet.id),
            self.raw_packet.buffer_size,
            self.raw_packet.handle_count,
        )
    }
}
