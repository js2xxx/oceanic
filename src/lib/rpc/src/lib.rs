#![no_std]
#![feature(iterator_try_collect)]

pub mod load;

extern crate alloc;

#[cfg(feature = "async")]
use core::future::Future;
use core::mem;

use solvent::prelude::Channel;
use solvent_rpc_core::packet::{self, Method};

/// # Safety
///
/// The implementor must be capable of byte-copying.
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
    type Request: Method;
    type Response: Method;
}

pub fn handle_blocking<T: Carrier, F>(channel: &Channel, proc: F) -> solvent::error::Result
where
    F: FnOnce(T::Request) -> T::Response,
{
    channel.handle(|packet| {
        let request = packet::deserialize(packet, None).map_err(|_| solvent::error::ETYPE)?;
        let response = proc(request);
        packet::serialize(response, packet).map_err(|_| solvent::error::EFAULT)?;
        Ok(())
    })
}

#[cfg(feature = "async")]
pub async fn handle<T: Carrier, F, G>(
    channel: &solvent_async::ipc::Channel,
    proc: G,
) -> solvent::error::Result
where
    G: FnOnce(T::Request) -> F,
    F: Future<Output = T::Response>,
{
    let fut = channel.handle(|mut packet| async {
        let request = packet::deserialize(&packet, None).map_err(|_| solvent::error::ETYPE)?;
        let response = proc(request).await;
        packet::serialize(response, &mut packet).map_err(|_| solvent::error::EFAULT)?;
        Ok(((), packet))
    });
    fut.await
}
