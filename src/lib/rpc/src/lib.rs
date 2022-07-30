#![no_std]
#![feature(iterator_try_collect)]

pub mod load;

extern crate alloc;

use alloc::{ffi::CString, vec::Vec};
use core::{
    ffi::{CStr, FromBytesWithNulError},
    mem,
    time::Duration,
};

use solvent::prelude::{Channel, PacketTyped};

/// # Safety
///
/// The implementor must be of the plain-old-data type.
pub unsafe trait Byted: Default {
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let mut ret = Self::default();
        let size = mem::size_of_val(&ret);
        if bytes.len() != size {
            None
        } else {
            ret.as_mut_bytes().copy_from_slice(bytes);
            Some(ret)
        }
    }

    fn as_bytes(&self) -> &[u8] {
        let ptr = self as *const _ as *const _;
        unsafe { core::slice::from_raw_parts(ptr, mem::size_of::<Self>()) }
    }

    fn as_mut_bytes(&mut self) -> &mut [u8] {
        let ptr = self as *mut _ as *mut _;
        unsafe { core::slice::from_raw_parts_mut(ptr, mem::size_of::<Self>()) }
    }
}

pub trait Carrier: Sized {
    type Request: PacketTyped;
    type Response: PacketTyped;
}

pub fn call<T: Carrier>(
    channel: &Channel,
    request: T::Request,
    timeout: Duration,
) -> solvent::error::Result<T::Response> {
    let mut packet = request.into_packet();
    channel.call(&mut packet, timeout)?;
    <T::Response>::try_from_packet(&mut packet).map_err(Into::into)
}

pub fn handle<T: Carrier, F>(channel: &Channel, proc: F) -> solvent::error::Result
where
    F: FnOnce(T::Request) -> T::Response,
{
    channel.handle(|packet| {
        let request = <T::Request>::try_from_packet(packet).map_err(Into::into)?;
        let response = proc(request);
        *packet = response.into_packet();
        Ok(())
    })
}

fn from_cstr_vec(data: Vec<CString>) -> Vec<u8> {
    data.into_iter().fold(Vec::new(), |mut acc, arg| {
        acc.append(&mut arg.into_bytes_with_nul());
        acc
    })
}

fn parse_cstr_vec(data: &[u8]) -> Result<Vec<CString>, FromBytesWithNulError> {
    data.split_inclusive(|&b| b == 0)
        .map(|data| CStr::from_bytes_with_nul(data).map(CString::from))
        .try_collect()
}
