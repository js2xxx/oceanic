use alloc::{alloc::Global, sync::Arc};
use core::alloc::{Allocator, Layout};

use paging::{LAddr, PAddr};

use super::Flags;

#[derive(Debug)]
pub struct Phys {
    from_allocator: bool,
    base: PAddr,
    layout: Layout,
    flags: Flags,
}

impl Phys {
    #[inline]
    pub fn new(base: PAddr, layout: Layout, flags: Flags) -> Arc<Phys> {
        unsafe { Arc::new(Self::new_manual(false, base, layout, flags)) }
    }

    /// # Errors
    ///
    /// Returns error if the heap memory is exhausted.
    pub fn allocate(layout: Layout, flags: Flags) -> solvent::Result<Arc<Phys>> {
        let layout = layout.align_to(paging::PAGE_LAYOUT.align())?.pad_to_align();
        let mem = if flags.contains(Flags::ZEROED) {
            Global.allocate_zeroed(layout)
        } else {
            Global.allocate(layout)
        };
        mem.map(|ptr| unsafe {
            Arc::new(Phys::new_manual(
                true,
                LAddr::from(ptr).to_paddr(minfo::ID_OFFSET),
                layout,
                flags,
            ))
        })
        .map_err(solvent::Error::from)
    }

    pub(super) unsafe fn new_manual(
        from_allocator: bool,
        base: PAddr,
        layout: Layout,
        flags: Flags,
    ) -> Phys {
        let layout = layout
            .align_to(paging::PAGE_LAYOUT.align())
            .expect("Unalignable layout");
        Phys {
            from_allocator,
            base,
            layout,
            flags,
        }
    }

    #[inline]
    pub fn base(&self) -> PAddr {
        self.base
    }

    #[inline]
    pub fn layout(&self) -> Layout {
        self.layout
    }

    #[inline]
    pub fn flags(&self) -> Flags {
        self.flags
    }

    #[inline]
    pub fn raw_ptr(&self) -> *mut u8 {
        *self.base.to_laddr(minfo::ID_OFFSET)
    }

    pub fn consume(this: Arc<Self>) -> PAddr {
        this.from_allocator.then(|| this.base).unwrap_or_default()
    }
}

impl Drop for Phys {
    fn drop(&mut self) {
        if self.from_allocator {
            let ptr = self.base.to_laddr(minfo::ID_OFFSET).as_non_null().unwrap();
            unsafe { Global.deallocate(ptr, self.layout) };
        }
    }
}
