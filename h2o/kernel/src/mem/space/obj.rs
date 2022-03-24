use alloc::alloc::Global;
use core::alloc::{Allocator, Layout};

use bitop_ex::BitOpEx;
use paging::{LAddr, PAddr, PAGE_SHIFT};

use super::Flags;
use crate::sched::Arsc;

#[derive(Debug)]
struct PhysInner {
    from_allocator: bool,
    base: PAddr,
    layout: Layout,
    flags: Flags,
}

impl PhysInner {
    unsafe fn new_manual(
        from_allocator: bool,
        base: PAddr,
        layout: Layout,
        flags: Flags,
    ) -> PhysInner {
        let layout = layout
            .align_to(paging::PAGE_LAYOUT.align())
            .expect("Unalignable layout");
        PhysInner {
            from_allocator,
            base,
            layout,
            flags,
        }
    }
}

impl Drop for PhysInner {
    fn drop(&mut self) {
        if self.from_allocator {
            let ptr = unsafe { self.base.to_laddr(minfo::ID_OFFSET).as_non_null_unchecked() };
            unsafe { Global.deallocate(ptr, self.layout) };
        }
    }
}

#[derive(Debug, Clone)]
pub struct Phys {
    offset: usize,
    len: usize,
    flags: Flags,
    inner: Arsc<PhysInner>,
}

impl From<Arsc<PhysInner>> for Phys {
    fn from(inner: Arsc<PhysInner>) -> Self {
        Phys {
            offset: 0,
            len: inner.layout.size(),
            flags: inner.flags,
            inner,
        }
    }
}

impl Phys {
    #[inline]
    pub fn new(base: PAddr, layout: Layout, flags: Flags) -> sv_call::Result<Self> {
        unsafe { Arsc::try_new(PhysInner::new_manual(false, base, layout, flags)) }
            .map_err(sv_call::Error::from)
            .map(Self::from)
    }

    /// # Errors
    ///
    /// Returns error if the heap memory is exhausted.
    pub fn allocate(layout: Layout, flags: Flags) -> sv_call::Result<Self> {
        let mut inner = Arsc::try_new_uninit()?;
        let layout = layout.align_to(paging::PAGE_LAYOUT.align())?.pad_to_align();
        let mem = if flags.contains(Flags::ZEROED) {
            Global.allocate_zeroed(layout)
        } else {
            Global.allocate(layout)
        };
        mem.map(|ptr| unsafe {
            Arsc::get_mut_unchecked(&mut inner).write(PhysInner::new_manual(
                true,
                LAddr::from(ptr).to_paddr(minfo::ID_OFFSET),
                layout,
                flags,
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

    pub fn flags(&self) -> Flags {
        self.flags
    }

    pub fn create_sub(&self, offset: usize, len: usize) -> sv_call::Result<Self> {
        if offset.contains_bit(PAGE_SHIFT) || len.contains_bit(PAGE_SHIFT) {
            return Err(sv_call::Error::EALIGN);
        }

        let offset = self.offset.wrapping_add(offset);
        let end = offset.wrapping_add(len);
        if self.offset <= offset && offset < end && end <= self.offset + self.len {
            Ok(Phys {
                offset,
                len,
                flags: self.flags,
                inner: Arsc::clone(&self.inner),
            })
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
