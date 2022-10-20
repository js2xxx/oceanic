use sv_call::{Handle, Result, SV_EVENT};

use crate::prelude::Object;

#[repr(transparent)]
#[derive(Debug)]
pub struct Event(Handle);

crate::impl_obj!(Event, SV_EVENT);
crate::impl_obj!(@CLONE, Event);
crate::impl_obj!(@DROP, Event);

impl Event {
    pub fn try_new(init_signal: usize) -> Result<Self> {
        let handle = unsafe { sv_call::sv_event_new(init_signal) }.into_res()?;
        // SAFETY: The handles are freshly allocated.
        Ok(unsafe { Event::from_raw(handle) })
    }

    #[inline]
    pub fn new(init_signal: usize) -> Self {
        Self::try_new(init_signal).expect("Failed to create event object")
    }

    pub fn notify(&self, clear: usize, set: usize) -> Result<usize> {
        // SAFETY: We don't move the ownership of the handle.
        let signal =
            unsafe { sv_call::sv_event_notify(unsafe { self.raw() }, clear, set) }.into_res()?;
        Ok(signal as usize)
    }

    pub fn cancel(&self) -> Result {
        // SAFETY: We don't move the ownership of the handle.
        unsafe { sv_call::sv_event_cancel(unsafe { self.raw() }) }.into_res()
    }
}
