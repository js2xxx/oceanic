#[cfg(feature = "alloc")]
use alloc::vec::Vec;
#[cfg(feature = "alloc")]
use core::slice;

use super::PAGE_SIZE;
use crate::{
    dev::MemRes,
    error::{Result, ERANGE},
    obj::{Object, private::Sealed},
};

pub struct Phys {
    inner: sv_call::Handle,
    len: usize,
}

impl Sealed for Phys {}
impl Object for Phys {
    unsafe fn raw(&self) -> sv_call::Handle {
        self.inner
    }

    unsafe fn from_raw(raw: sv_call::Handle) -> Self {
        let len = unsafe { sv_call::sv_phys_size(raw) }
            .into_res()
            .expect("Failed to get the size of the physical object") as usize;
        Phys { inner: raw, len }
    }
}

crate::impl_obj!(@CLONE, Phys);
crate::impl_obj!(@DROP, Phys);

impl Phys {
    pub fn allocate(size: usize, zeroed: bool) -> Result<Self> {
        let len = size.next_multiple_of(PAGE_SIZE);
        let inner = unsafe { sv_call::sv_phys_alloc(len, zeroed) }.into_res()?;
        // SAFETY: The handle is freshly allocated.
        Ok(Phys { len, inner })
    }

    pub fn acquire(res: &MemRes, addr: usize, size: usize) -> Result<Self> {
        let len = size.next_multiple_of(PAGE_SIZE);
        let inner = unsafe {
            sv_call::sv_phys_acq(
                // SAFETY: We don't move the ownership of the memory resource handle.
                unsafe { res.raw() },
                addr,
                len,
            )
            .into_res()?
        };
        // SAFETY: The handle is freshly allocated.
        Ok(Phys { len, inner })
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
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

    pub fn create_sub(&self, offset: usize, len: usize, copy: bool) -> Result<Self> {
        if len > 0 {
            let handle = unsafe { sv_call::sv_phys_sub(unsafe { self.raw() }, offset, len, copy) }
                .into_res()?;
            Ok(Phys { len, inner: handle })
        } else {
            Err(ERANGE)
        }
    }
}
