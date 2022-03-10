use alloc::vec::Vec;
use core::{alloc::Layout, ptr::NonNull, slice};

pub use sv_call::mem::Flags;

use crate::{error::Result, obj::Object};

cfg_if::cfg_if! { if #[cfg(target_arch = "x86_64")] {

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;

}}
pub const PAGE_LAYOUT: Layout = unsafe { Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE) };

pub struct Phys(sv_call::Handle);

crate::impl_obj!(Phys);
crate::impl_obj!(@CLONE, Phys);
crate::impl_obj!(@DROP, Phys);

impl Phys {
    pub fn allocate(layout: Layout, flags: Flags) -> Result<Self> {
        let layout = layout.align_to(PAGE_LAYOUT.align())?.pad_to_align();
        sv_call::sv_phys_alloc(layout.size(), layout.align(), flags).into_res()
            // SAFETY: The handle is freshly allocated.
            .map(|handle| unsafe { Self::from_raw(handle) })
    }

    pub fn read_into(&self, offset: usize, buffer: &mut [u8]) -> Result {
        sv_call::sv_phys_read(
            // SAFETY: We don't move the ownership of the handle.
            unsafe { self.raw() },
            offset,
            buffer.len(),
            buffer.as_mut_ptr(),
        )
        .into_res()
    }

    pub fn read(&self, offset: usize, len: usize) -> Result<Vec<u8>> {
        let mut ret = Vec::with_capacity(len);
        unsafe {
            let slice = slice::from_raw_parts_mut(ret.as_mut_ptr(), len);
            self.read_into(offset, slice)?;
            ret.set_len(len);
        }
        Ok(ret)
    }

    /// # Safety
    ///
    /// The caller must guarantee the memory safety of sharing the object.
    pub unsafe fn write(&self, offset: usize, buffer: &[u8]) -> Result {
        // SAFETY: We don't move the ownership of the handle.
        sv_call::sv_phys_write(unsafe { self.raw() }, offset, buffer.len(), buffer.as_ptr())
            .into_res()
    }
}

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
