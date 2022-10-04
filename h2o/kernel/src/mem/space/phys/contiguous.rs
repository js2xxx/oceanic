use alloc::alloc::Global;
use core::{
    alloc::{Allocator, Layout},
    slice,
};

use bitop_ex::BitOpEx;
use paging::{LAddr, PAddr, PAGE_SHIFT, PAGE_SIZE};
use sv_call::Result;

use crate::{
    sched::Arsc,
    syscall::{In, Out, UserPtr},
};

#[derive(Debug)]
struct PhysInner {
    from_allocator: bool,
    base: PAddr,
    size: usize,
}

impl PhysInner {
    unsafe fn new_manual(from_allocator: bool, base: PAddr, size: usize) -> PhysInner {
        PhysInner {
            from_allocator,
            base,
            size,
        }
    }
}

impl Drop for PhysInner {
    fn drop(&mut self) {
        if self.from_allocator {
            let ptr = unsafe { self.base.to_laddr(minfo::ID_OFFSET).as_non_null_unchecked() };
            let layout =
                unsafe { Layout::from_size_align_unchecked(self.size, PAGE_SIZE) }.pad_to_align();
            unsafe { Global.deallocate(ptr, layout) };
        }
    }
}

#[derive(Debug, Clone)]
pub struct Phys {
    offset: usize,
    len: usize,
    inner: Arsc<PhysInner>,
}

pub type PinnedPhys = Phys;

impl From<Arsc<PhysInner>> for Phys {
    fn from(inner: Arsc<PhysInner>) -> Self {
        Phys {
            offset: 0,
            len: inner.size,
            inner,
        }
    }
}

impl Phys {
    #[inline]
    pub fn new(base: PAddr, size: usize) -> Result<Self> {
        unsafe { Arsc::try_new(PhysInner::new_manual(false, base, size)) }
            .map_err(sv_call::Error::from)
            .map(Self::from)
    }

    /// # Errors
    ///
    /// Returns error if the heap memory is exhausted or the size is zero.
    pub fn allocate(size: usize, zeroed: bool) -> Result<Self> {
        if size == 0 {
            return Err(sv_call::ENOMEM);
        }

        let mut inner = Arsc::try_new_uninit()?;
        let layout = unsafe { Layout::from_size_align_unchecked(size, PAGE_SIZE) }.pad_to_align();
        let mem = if zeroed {
            Global.allocate_zeroed(layout)
        } else {
            Global.allocate(layout)
        };

        mem.map(|ptr| unsafe {
            Arsc::get_mut_unchecked(&mut inner).write(PhysInner::new_manual(
                true,
                LAddr::from(ptr).to_paddr(minfo::ID_OFFSET),
                size,
            ));
            Arsc::assume_init(inner)
        })
        .map_err(sv_call::Error::from)
        .map(Self::from)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn pin(this: Self) -> PinnedPhys {
        this
    }

    pub fn create_sub(&self, offset: usize, len: usize, copy: bool) -> Result<Self> {
        if offset.contains_bit(PAGE_SHIFT) || len.contains_bit(PAGE_SHIFT) {
            return Err(sv_call::EALIGN);
        }

        let new_offset = self.offset.wrapping_add(offset);
        let end = new_offset.wrapping_add(len);
        if self.offset <= new_offset && new_offset < end && end <= self.offset + self.len {
            if copy {
                let child = Self::allocate(len, true)?;
                let dst = child.raw();
                unsafe {
                    let src = self.raw().add(offset);
                    dst.copy_from_nonoverlapping(src, len);
                }
                Ok(child)
            } else {
                Ok(Phys {
                    offset: new_offset,
                    len,
                    inner: Arsc::clone(&self.inner),
                })
            }
        } else {
            Err(sv_call::ERANGE)
        }
    }

    pub fn base(&self) -> PAddr {
        PAddr::new(*self.inner.base + self.offset)
    }

    #[inline]
    pub fn map_iter(&self, offset: usize, len: usize) -> impl Iterator<Item = (PAddr, usize)> {
        let base = PAddr::new(*self.inner.base + self.offset + offset);
        let len = self.len.saturating_sub(offset).min(len);
        (len > 0).then_some((base, len)).into_iter()
    }

    fn raw(&self) -> *mut u8 {
        unsafe { self.inner.base.to_laddr(minfo::ID_OFFSET).add(self.offset) }
    }

    pub fn read(&self, offset: usize, len: usize, buffer: UserPtr<Out, u8>) -> Result<usize> {
        let offset = self.len.min(offset);
        let len = self.len.saturating_sub(offset).min(len);
        unsafe {
            let ptr = self.raw().add(offset);
            let slice = slice::from_raw_parts(ptr, len);
            buffer.write_slice(slice)?;
        }
        Ok(len)
    }

    pub fn write(&self, offset: usize, len: usize, buffer: UserPtr<In, u8>) -> Result<usize> {
        let offset = self.len.min(offset);
        let len = self.len.saturating_sub(offset).min(len);
        unsafe {
            let ptr = self.raw().add(offset);
            buffer.read_slice(ptr, len)?;
        }
        Ok(len)
    }
}

impl PartialEq for Phys {
    fn eq(&self, other: &Self) -> bool {
        self.offset == other.offset
            && self.len == other.len
            && Arsc::ptr_eq(&self.inner, &other.inner)
    }
}
