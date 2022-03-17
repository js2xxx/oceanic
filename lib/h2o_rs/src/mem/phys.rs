use alloc::vec::Vec;
use core::{alloc::Layout, slice};

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

    pub fn into_ref(self, len: usize) -> PhysRef {
        PhysRef {
            phys: self,
            offset: 0,
            len,
        }
    }

    pub fn read_into(&self, offset: usize, buffer: &mut [u8]) -> Result {
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
    }

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
        // SAFETY: We don't move the ownership of the handle.
        sv_call::sv_phys_write(unsafe { self.raw() }, offset, buffer.len(), buffer.as_ptr())
            .into_res()
    }
}

#[derive(Clone)]
pub struct PhysRef {
    phys: Phys,
    offset: usize,
    len: usize,
}

impl PhysRef {
    pub fn phys(&self) -> &Phys {
        &self.phys
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn into_parts(self) -> (Phys, usize, usize) {
        (self.phys, self.offset, self.len)
    }

    fn check_range(&self, offset: usize, len: usize) -> Option<usize> {
        let offset = self.offset.checked_add(offset)?;
        let end = offset.checked_add(len)?;
        (end < self.offset + self.len).then(|| offset)
    }

    pub fn dup_sub(&self, offset: usize, len: usize) -> Option<Self> {
        self.check_range(offset, len).map(|offset| PhysRef {
            phys: Phys::clone(&self.phys),
            offset,
            len,
        })
    }

    pub fn read_into(&self, offset: usize, buffer: &mut [u8]) -> Result {
        let offset = self
            .check_range(offset, buffer.len())
            .ok_or(Error::ERANGE)?;
        self.phys.read_into(offset, buffer)
    }

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
        let offset = self
            .check_range(offset, buffer.len())
            .ok_or(Error::ERANGE)?;
        // SAFETY: The safety condition is guaranteed by the caller.
        unsafe { self.phys.write(offset, buffer) }
    }
}
