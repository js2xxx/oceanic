mod chan;
#[cfg(feature = "alloc")]
mod packet;

use core::time::Duration;

pub use sv_call::ipc::*;

pub use self::chan::Channel;
#[cfg(feature = "alloc")]
pub use self::packet::*;
use crate::{error::Result, obj::Object};

#[repr(transparent)]
pub struct Blocker(sv_call::Handle);
crate::impl_obj!(Blocker);
crate::impl_obj!(@DROP, Blocker);

impl Blocker {
    pub fn end_wait(self, timeout: Duration) -> Result<usize> {
        unsafe {
            sv_call::sv_obj_awend(Blocker::into_raw(self), crate::time::try_into_us(timeout)?)
                .into_res()
                .map(|value| value as usize)
        }
    }
}

#[repr(transparent)]
pub struct Dispatcher(sv_call::Handle);
crate::impl_obj!(Dispatcher);
crate::impl_obj!(@CLONE, Dispatcher);
crate::impl_obj!(@DROP, Dispatcher);

impl Dispatcher {
    pub fn try_new() -> Result<Self> {
        let handle = unsafe { sv_call::sv_disp_new() }.into_res()?;
        Ok(unsafe { Self::from_raw(handle) })
    }

    #[inline]
    pub fn new() -> Self {
        Self::try_new().expect("Failed to create object dispatcher")
    }

    pub fn push(&self, obj: &impl Object, level_triggered: bool, signal: usize) -> Result<usize> {
        let key = unsafe {
            let disp = unsafe { self.raw() };
            sv_call::sv_obj_await2(unsafe { obj.raw() }, level_triggered, signal, disp)
        }
        .into_res()?;
        Ok(key as usize)
    }

    pub fn pop(&self) -> Result<(usize, bool)> {
        let mut canceled = false;
        let key =
            unsafe { sv_call::sv_obj_awend2(unsafe { self.raw() }, &mut canceled) }.into_res()?;
        Ok((key as usize, canceled))
    }
}

impl Default for Dispatcher {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
