#[cfg(feature = "alloc")]
use alloc::vec::Vec;
#[cfg(feature = "alloc")]
use core::slice;
use core::{alloc::Layout, ops::Deref};

use super::{Flags, PAGE_LAYOUT};
use crate::{
    dev::MemRes,
    error::{Error, Result},
    obj::Object,
};

#[repr(transparent)]
pub struct Phys(sv_call::Handle);

crate::impl_obj!(Phys);
crate::impl_obj!(@CLONE, Phys);
crate::impl_obj!(@DROP, Phys);

impl Phys {
    pub fn allocate(layout: Layout, flags: Flags) -> Result<Self> {
        let layout = layout.align_to(PAGE_LAYOUT.align())?.pad_to_align();
        unsafe {
            sv_call::sv_phys_alloc(layout.size(), layout.align(), flags).into_res()
            // SAFETY: The handle is freshly allocated.
            .map(|handle| unsafe { Self::from_raw(handle) })
        }
    }

    pub fn acquire(res: &MemRes, addr: usize, layout: Layout, flags: Flags) -> Result<Self> {
        let layout = layout.align_to(PAGE_LAYOUT.align())?.pad_to_align();
        let handle = unsafe {
            sv_call::sv_phys_acq(
                // SAFETY: We don't move the ownership of the memory resource handle.
                unsafe { res.raw() },
                addr,
                layout.size(),
                layout.align(),
                flags,
            )
            .into_res()?
        };
        // SAFETY: The handle is freshly allocated.
        Ok(unsafe { Self::from_raw(handle) })
    }

    pub fn try_get_len(&self) -> Result<usize> {
        // SAFETY: We don't move the ownership of the handle.
        unsafe { sv_call::sv_phys_size(unsafe { self.raw() }) }
            .into_res()
            .map(|value| value as usize)
    }

    pub fn len(&self) -> usize {
        self.try_get_len()
            .expect("Failed to get the size of the physical object")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn into_ref(self) -> PhysRef {
        PhysRef {
            len: self.len(),
            phys: self,
        }
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

    pub fn create_sub(&self, offset: usize, len: usize) -> Result<Self> {
        if len > 0 {
            let handle =
                unsafe { sv_call::sv_phys_sub(unsafe { self.raw() }, offset, len) }.into_res()?;
            Ok(unsafe { Self::from_raw(handle) })
        } else {
            Err(Error::ERANGE)
        }
    }
}

#[derive(Clone)]
pub struct PhysRef {
    phys: Phys,
    len: usize,
}

impl PhysRef {
    pub fn phys(&self) -> &Phys {
        &self.phys
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn into_inner(self) -> Phys {
        self.phys
    }
}

impl Deref for PhysRef {
    type Target = Phys;

    fn deref(&self) -> &Self::Target {
        &self.phys
    }
}
