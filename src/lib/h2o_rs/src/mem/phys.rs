#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
use core::num::NonZeroUsize;
#[cfg(feature = "alloc")]
use core::slice;

pub use sv_call::mem::PhysOptions;
use sv_call::{
    c_ty::{Status, StatusOrValue},
    mem::IoVec,
    Syscall, EAGAIN, EINVAL, SV_PHYS,
};

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
    pub fn allocate(size: usize, options: PhysOptions) -> Result<Self> {
        let handle = unsafe { sv_call::sv_phys_alloc(size, options) }.into_res()?;
        // SAFETY: The handle is freshly allocated.
        Ok(unsafe { Self::from_raw(handle) })
    }

    pub fn acquire(res: &MemRes, addr: Option<NonZeroUsize>, size: usize) -> Result<Self> {
        let len = size.next_multiple_of(PAGE_SIZE);
        let handle = unsafe {
            sv_call::sv_phys_acq(
                // SAFETY: We don't move the ownership of the memory resource handle.
                unsafe { res.raw() },
                addr.map_or(0, |addr| addr.get()),
                len,
            )
            .into_res()?
        };
        // SAFETY: The handle is freshly allocated.
        Ok(unsafe { Self::from_raw(handle) })
    }

    /// # Note
    ///
    /// This function is rather expensive and is not preferred for frequent use.
    /// Also, for resizable objects, the result may be inconsistent and should
    /// just be used for hinting like pre-allocating buffers for R/W operations.
    ///
    /// FIXME: See the implementation in the kernel.
    pub fn len(&self) -> usize {
        unsafe { sv_call::sv_phys_size(unsafe { self.raw() }) }
            .into_res()
            .expect("Failed to get the size of the physical object") as usize
    }

    /// # Note
    ///
    /// See `Phys::len` for more info.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn read_into(&self, offset: usize, buffer: &mut [u8]) -> Result<usize> {
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
                .map(|len| len as usize)
            }
        } else {
            Ok(0)
        }
    }

    #[cfg(feature = "alloc")]
    pub fn read(&self, offset: usize, len: usize) -> Result<Vec<u8>> {
        let mut ret = Vec::with_capacity(len);
        // SAFETY: The content is from the object, guaranteed to be valid.
        unsafe {
            let slice = slice::from_raw_parts_mut(ret.as_mut_ptr(), len);
            let len = self.read_into(offset, slice)?;
            ret.set_len(len);
        }
        Ok(ret)
    }

    /// # Safety
    ///
    /// The caller must guarantee the memory safety of sharing the object if the
    /// object is contiguous, or the object is mapped to a different address.
    ///
    /// Note: If the object is not contiguous, and it is not mapped to any
    /// address, the kernel will guarantee its memory safety.
    pub unsafe fn write(&self, offset: usize, buffer: &[u8]) -> Result<usize> {
        if !buffer.is_empty() {
            // SAFETY: We don't move the ownership of the handle.
            sv_call::sv_phys_write(unsafe { self.raw() }, offset, buffer.len(), buffer.as_ptr())
                .into_res()
                .map(|len| len as usize)
        } else {
            Ok(0)
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

    #[cfg(feature = "alloc")]
    pub fn pack_read(&self, offset: usize, mut buf: Vec<u8>) -> PackRead {
        buf.clear();
        let cap = buf.spare_capacity_mut();
        let raw_buf = Box::new(IoVec {
            ptr: cap.as_mut_ptr() as _,
            len: cap.len(),
        });
        let syscall =
            unsafe { sv_call::sv_pack_phys_readv(unsafe { self.raw() }, offset, &*raw_buf, 1) };
        PackRead {
            raw_buf,
            buf,
            syscall,
        }
    }

    /// # Safety
    ///
    /// The caller must guarantee the memory safety of sharing the object if the
    /// object is contiguous, or the object is mapped to a different address.
    ///
    /// Note: If the object is not contiguous, and it is not mapped to any
    /// address, the kernel will guarantee its memory safety.
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

    #[cfg(feature = "alloc")]
    pub fn pack_write(&self, offset: usize, buf: Vec<u8>) -> PackWrite {
        let raw_buf = Box::new(IoVec {
            ptr: buf.as_ptr() as *mut u8,
            len: buf.len(),
        });
        let syscall =
            unsafe { sv_call::sv_pack_phys_writev(unsafe { self.raw() }, offset, &*raw_buf, 1) };
        PackWrite {
            raw_buf,
            buf,
            syscall,
        }
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

    #[inline]
    pub fn pack_resize(&self, new_len: usize, zeroed: bool) -> PackResize {
        PackResize(unsafe { sv_call::sv_pack_phys_resize(unsafe { self.raw() }, new_len, zeroed) })
    }
}

#[cfg(feature = "alloc")]
pub struct PackRead {
    pub raw_buf: Box<IoVec>,
    pub buf: Vec<u8>,
    pub syscall: Syscall,
}

#[cfg(feature = "alloc")]
unsafe impl Send for PackRead {}

#[cfg(feature = "alloc")]
impl PackRead {
    pub fn receive(&mut self, res: StatusOrValue, canceled: bool) -> Result<usize> {
        res.into_res().and((!canceled).then_some(0).ok_or(EAGAIN))
    }
}

#[cfg(feature = "alloc")]
pub struct PackWrite {
    pub raw_buf: Box<IoVec>,
    pub buf: Vec<u8>,
    pub syscall: Syscall,
}

#[cfg(feature = "alloc")]
unsafe impl Send for PackWrite {}

#[cfg(feature = "alloc")]
impl PackWrite {
    pub fn receive(&mut self, res: StatusOrValue, canceled: bool) -> Result<usize> {
        res.into_res().and((!canceled).then_some(0).ok_or(EAGAIN))
    }
}

#[cfg(feature = "alloc")]
pub struct PackResize(pub Syscall);

#[cfg(feature = "alloc")]
unsafe impl Send for PackResize {}

#[cfg(feature = "alloc")]
impl PackResize {
    pub fn receive(&mut self, res: Status, canceled: bool) -> Result {
        res.into_res().and((!canceled).then_some(()).ok_or(EAGAIN))
    }
}
