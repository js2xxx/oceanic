mod chan;

use core::time::Duration;

pub use self::chan::{Channel, Packet};

use crate::{error::Result, obj::Object};

#[repr(transparent)]
pub struct Waiter(sv_call::Handle);
crate::impl_obj!(Waiter);
crate::impl_obj!(@DROP, Waiter);

impl Waiter {
    pub fn end_wait(self, timeout: Duration) -> Result<usize> {
        sv_call::sv_obj_awend(Waiter::into_raw(self), u64::try_from(timeout.as_micros())?)
            .into_res()
            .map(|value| value as usize)
    }
}
