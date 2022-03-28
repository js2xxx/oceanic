use alloc::alloc::Global;
use core::alloc::{Allocator, Layout};

use bitop_ex::BitOpEx;
use paging::{LAddr, PAddr, PAGE_SHIFT, PAGE_SIZE};
use sv_call::{Feature, Result};

use crate::sched::{task::hdl::DefaultFeature, Arsc};

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
    /// Returns error if the heap memory is exhausted.
    pub fn allocate(size: usize, zeroed: bool) -> Result<Self> {
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

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn create_sub(&self, offset: usize, len: usize, copy: bool) -> Result<Self> {
        if offset.contains_bit(PAGE_SHIFT) || len.contains_bit(PAGE_SHIFT) {
            return Err(sv_call::Error::EALIGN);
        }

        let new_offset = self.offset.wrapping_add(offset);
        let end = new_offset.wrapping_add(len);
        if self.offset <= new_offset && new_offset < end && end <= self.offset + self.len {
            if copy {
                let child = Self::allocate(len, true)?;
                let dst = child.raw_ptr();
                unsafe {
                    let src = self.raw_ptr().add(offset);
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
            Err(sv_call::Error::ERANGE)
        }
    }

    pub fn base(&self) -> PAddr {
        PAddr::new(*self.inner.base + self.offset)
    }

    pub fn raw_ptr(&self) -> *mut u8 {
        unsafe { self.inner.base.to_laddr(minfo::ID_OFFSET).add(self.offset) }
    }
}

impl PartialEq for Phys {
    fn eq(&self, other: &Self) -> bool {
        self.offset == other.offset
            && self.len == other.len
            && Arsc::ptr_eq(&self.inner, &other.inner)
    }
}

unsafe impl DefaultFeature for Phys {
    fn default_features() -> Feature {
        Feature::SEND | Feature::SYNC | Feature::READ | Feature::WRITE | Feature::EXECUTE
    }
}
