#[cfg(feature = "alloc")]
use alloc::vec::Vec;
#[cfg(feature = "alloc")]
use core::slice;

use sv_call::{EINVAL, SV_PHYS};

use super::{IoSlice, IoSliceMut, PAGE_SIZE};
use crate::{
    dev::MemRes,
    error::{Result, ERANGE},
    obj::Object,
};

#[derive(Debug)]
#[repr(transparent)]
pub struct Phys(sv_call::Handle);

crate::impl_obj!(Phys, SV_PHYS);
crate::impl_obj!(@CLONE, Phys);
crate::impl_obj!(@DROP, Phys);

impl Phys {
    pub fn allocate(size: usize, zeroed: bool) -> Result<Self> {
        let len = size.next_multiple_of(PAGE_SIZE);
        let handle = unsafe { sv_call::sv_phys_alloc(len, zeroed) }.into_res()?;
        // SAFETY: The handle is freshly allocated.
        Ok(unsafe { Self::from_raw(handle) })
    }

    pub fn acquire(res: &MemRes, addr: usize, size: usize) -> Result<Self> {
        let len = size.next_multiple_of(PAGE_SIZE);
        let handle = unsafe {
            sv_call::sv_phys_acq(
                // SAFETY: We don't move the ownership of the memory resource handle.
                unsafe { res.raw() },
                addr,
                len,
            )
            .into_res()?
        };
        // SAFETY: The handle is freshly allocated.
        Ok(unsafe { Self::from_raw(handle) })
    }

    pub fn len(&self) -> usize {
        unsafe { sv_call::sv_phys_size(unsafe { self.raw() }) }
            .into_res()
            .expect("Failed to get the size of the physical object") as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn read_into(&self, offset: usize, buffer: &mut [u8]) -> Result {
        if !buffer.is_empty() {
            unsafe {
                sv_call::sv_phys_read(
                    // SAFETY: We don't move the ownership of the handle.
                    unsafe { self.raw() },
                    offset,
                    buffer.len(),
                    buffer.as_mut_ptr(),
                )
                .into_res()
            }
        } else {
            Ok(())
        }
    }

    #[cfg(feature = "alloc")]
    pub fn read(&self, offset: usize, len: usize) -> Result<Vec<u8>> {
        let mut ret = Vec::with_capacity(len);
        // SAFETY: The content is from the object, guaranteed to be valid.
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
        if !buffer.is_empty() {
            // SAFETY: We don't move the ownership of the handle.
            sv_call::sv_phys_write(unsafe { self.raw() }, offset, buffer.len(), buffer.as_ptr())
                .into_res()
        } else {
            Ok(())
        }
    }

    pub fn read_vectored(&self, offset: usize, bufs: &mut [IoSliceMut<'_>]) -> Result<usize> {
        let len = unsafe {
            sv_call::sv_phys_readv(
                unsafe { self.raw() },
                offset,
                bufs.as_ptr() as _,
                bufs.len(),
            )
        }
        .into_res()?;
        Ok(len as usize)
    }

    /// # Safety
    ///
    /// The caller must guarantee the memory safety of sharing the object.
    pub unsafe fn write_vectored(&self, offset: usize, bufs: &[IoSlice<'_>]) -> Result<usize> {
        let len = unsafe {
            sv_call::sv_phys_writev(
                unsafe { self.raw() },
                offset,
                bufs.as_ptr() as _,
                bufs.len(),
            )
        }
        .into_res()?;
        Ok(len as usize)
    }

    pub fn create_sub(&self, offset: usize, len: usize, copy: bool) -> Result<Self> {
        if len > 0 {
            let handle = unsafe { sv_call::sv_phys_sub(unsafe { self.raw() }, offset, len, copy) }
                .into_res()?;
            Ok(unsafe { Self::from_raw(handle) })
        } else {
            Err(ERANGE)
        }
    }

    pub fn resize(&self, new_len: usize, zeroed: bool) -> Result {
        if new_len > 0 {
            unsafe { sv_call::sv_phys_resize(unsafe { self.raw() }, new_len, zeroed) }
                .into_res()?;
            Ok(())
        } else {
            Err(EINVAL)
        }
    }
}
