mod chan;
#[cfg(feature = "alloc")]
mod packet;

use core::time::Duration;

pub use self::chan::Channel;
#[cfg(feature = "alloc")]
pub use self::packet::*;
use crate::{error::Result, obj::Object};

#[repr(transparent)]
pub struct Waiter(sv_call::Handle);
crate::impl_obj!(Waiter);
crate::impl_obj!(@DROP, Waiter);

impl Waiter {
    pub fn end_wait(self, timeout: Duration) -> Result<usize> {
        unsafe {
            sv_call::sv_obj_awend(Waiter::into_raw(self), crate::time::try_into_us(timeout)?)
                .into_res()
                .map(|value| value as usize)
        }
    }
}
