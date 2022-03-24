use alloc::alloc::Global;
use core::alloc::{Allocator, Layout};

use bitop_ex::BitOpEx;
use paging::{LAddr, PAddr, PAGE_SHIFT};

use super::Flags;
use crate::sched::Arsc;

#[derive(Debug)]
pub struct PhysInner {
    from_allocator: bool,
    base: PAddr,
    layout: Layout,
    flags: Flags,
}

impl PhysInner {
    #[inline]
    fn new(base: PAddr, layout: Layout, flags: Flags) -> sv_call::Result<Arsc<PhysInner>> {
        unsafe { Arsc::try_new(Self::new_manual(false, base, layout, flags)) }
            .map_err(sv_call::Error::from)
    }

    /// # Errors
    ///
    /// Returns error if the heap memory is exhausted.
    fn allocate(layout: Layout, flags: Flags) -> sv_call::Result<Arsc<PhysInner>> {
        let mut phys = Arsc::try_new_uninit()?;
        let layout = layout.align_to(paging::PAGE_LAYOUT.align())?.pad_to_align();
        let mem = if flags.contains(Flags::ZEROED) {
            Global.allocate_zeroed(layout)
        } else {
            Global.allocate(layout)
        };
        mem.map(|ptr| unsafe {
            Arsc::get_mut_unchecked(&mut phys).write(PhysInner::new_manual(
                true,
                LAddr::from(ptr).to_paddr(minfo::ID_OFFSET),
                layout,
                flags,
            ));
            Arsc::assume_init(phys)
        })
        .map_err(sv_call::Error::from)
    }

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
    phys: Arsc<PhysInner>,
}

impl From<Arsc<PhysInner>> for Phys {
    fn from(phys: Arsc<PhysInner>) -> Self {
        Phys {
            len: phys.layout.size(),
            offset: 0,
            phys,
        }
    }
}

impl Phys {
    #[inline]
    pub fn new(base: PAddr, layout: Layout, flags: Flags) -> sv_call::Result<Self> {
        PhysInner::new(base, layout, flags).map(Self::from)
    }

    /// # Errors
    ///
    /// Returns error if the heap memory is exhausted.
    pub fn allocate(layout: Layout, flags: Flags) -> sv_call::Result<Self> {
        PhysInner::allocate(layout, flags).map(Self::from)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn flags(&self) -> Flags {
        self.phys.flags
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
                phys: Arsc::clone(&self.phys),
            })
        } else {
            Err(sv_call::Error::ERANGE)
        }
    }

    pub fn base(&self) -> PAddr {
        PAddr::new(*self.phys.base + self.offset)
    }

    pub fn raw_ptr(&self) -> *mut u8 {
        unsafe { self.phys.base.to_laddr(minfo::ID_OFFSET).add(self.offset) }
    }
}

impl PartialEq for Phys {
    fn eq(&self, other: &Self) -> bool {
        self.offset == other.offset
            && self.len == other.len
            && Arsc::ptr_eq(&self.phys, &other.phys)
    }
}
