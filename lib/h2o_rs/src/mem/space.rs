use core::ptr::NonNull;

use super::{Flags, Phys};
use crate::{error::Result, obj::Object};

#[repr(transparent)]
pub struct Space(sv_call::Handle);

crate::impl_obj!(Space);
crate::impl_obj!(@CLONE, Space);

impl Space {
    pub fn try_new() -> Result<Self> {
        // SAFETY: The handle is freshly allocated.
        sv_call::sv_space_new()
            .into_res()
            .map(|handle| unsafe { Self::from_raw(handle) })
    }

    pub fn new() -> Self {
        Self::try_new().expect("Failed to create task space")
    }

    pub fn current() -> Self {
        // SAFETY: The NULL handle represents the current task space.
        unsafe { Self::from_raw(sv_call::Handle::NULL) }
    }

    pub fn map(
        &self,
        addr: Option<usize>,
        phys: &Phys,
        phys_offset: usize,
        len: usize,
        flags: Flags,
    ) -> Result<NonNull<[u8]>> {
        let mi = sv_call::mem::MapInfo {
            addr: addr.unwrap_or_default(),
            map_addr: addr.is_some(),
            // SAFETY: The kernel create another implicit reference of `phys`.
            phys: unsafe { phys.raw() },
            phys_offset,
            len,
            flags,
        };
        // SAFETY: We don't move the ownership of the handle.
        sv_call::sv_mem_map(unsafe { self.raw() }, &mi)
            .into_res()
            .map(|ptr| {
                // SAFETY: The pointer returned is always non-null.
                let ptr = unsafe { NonNull::new_unchecked(ptr as *mut u8) };
                NonNull::slice_from_raw_parts(ptr, len)
            })
    }

    /// # Safety
    ///
    /// The pointer must be allocated from this space and must not be used
    /// anymore.
    pub unsafe fn unmap(&self, ptr: NonNull<u8>) -> Result {
        // SAFETY: We don't move the ownership of the handle.
        sv_call::sv_mem_unmap(unsafe { self.raw() }, ptr.as_ptr()).into_res()
    }

    /// # Safety
    ///
    /// The pointer must be allocated from this space and must not be used
    /// improperly anymore.
    pub unsafe fn reprotect(&self, ptr: NonNull<[u8]>, flags: Flags) -> Result {
        sv_call::sv_mem_reprot(
            // SAFETY: We don't move the ownership of the handle.
            unsafe { self.raw() },
            ptr.as_non_null_ptr().as_ptr(),
            ptr.len(),
            flags,
        )
        .into_res()
    }
}

impl Default for Space {
    fn default() -> Self {
        Self::current()
    }
}

impl Drop for Space {
    fn drop(&mut self) {
        // SAFETY: The object is guaranteed not to be used anymore.
        let handle = unsafe { self.raw() };
        if !handle.is_null() {
            // SAFETY: Calling in the drop context.
            unsafe { Object::try_drop(self) }.expect("Failed to drop task space")
        }
    }
}
