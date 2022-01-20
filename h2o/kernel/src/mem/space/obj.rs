use alloc::{alloc::Global, sync::Arc};
use core::{
    alloc::{Allocator, Layout},
    mem,
    ops::{Deref, Range},
    ptr::NonNull,
};

use paging::{LAddr, PAddr};

use super::{Flags, Space};
use crate::sched::{ipc::Arsc, task::Type};

#[derive(Debug)]
pub struct Phys {
    from_allocator: bool,
    base: PAddr,
    layout: Layout,
    flags: Flags,
}

impl Phys {
    pub fn new(base: PAddr, layout: Layout, flags: Flags) -> Arc<Phys> {
        unsafe { Arc::new(Self::new_manual(false, base, layout, flags)) }
    }

    /// # Errors
    ///
    /// Returns error if the heap memory is exhausted.
    pub fn allocate(layout: Layout, flags: Flags) -> solvent::Result<Arc<Phys>> {
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

    pub fn base(&self) -> PAddr {
        self.base
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn flags(&self) -> Flags {
        self.flags
    }

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

#[derive(Debug)]
pub struct Virt {
    ty: Type,
    ptr: NonNull<[u8]>,
    phys: Arc<Phys>,
    space: Arsc<Space>,
}

impl Virt {
    pub(super) fn new(ty: Type, ptr: NonNull<[u8]>, phys: Arc<Phys>, space: Arsc<Space>) -> Self {
        Virt {
            ty,
            ptr,
            phys,
            space,
        }
    }

    pub fn ty(&self) -> Type {
        self.ty
    }

    pub fn base(&self) -> LAddr {
        LAddr::new(self.ptr.as_mut_ptr())
    }

    pub fn as_ptr(&self) -> NonNull<[u8]> {
        self.ptr
    }

    pub fn range(&self) -> Range<LAddr> {
        let (ptr, len) = (self.ptr.as_mut_ptr(), self.ptr.len());
        self.base()..LAddr::new(unsafe { ptr.add(len) })
    }

    pub fn layout(&self) -> Layout {
        self.phys.layout
    }

    pub fn phys_flags(&self) -> Flags {
        self.phys.flags
    }

    /// # Errors
    ///
    /// Returns error if caller tries to support more features or the pointer is
    /// out of bounds.
    pub unsafe fn modify(&self, ptr: NonNull<[u8]>, flags: Flags) -> solvent::Result {
        if flags & !self.phys_flags() != Flags::empty() {
            return Err(solvent::Error::EPERM);
        }
        let (base, len) = (ptr.as_non_null_ptr(), ptr.len());
        let base = if base.as_ptr() >= *self.base() {
            base
        } else {
            return Err(solvent::Error::EINVAL);
        };
        let len = if base.as_ptr().add(len) <= *self.range().end {
            len
        } else {
            return Err(solvent::Error::EINVAL);
        };

        let ptr = NonNull::slice_from_raw_parts(base, len);
        self.space.modify(ptr, flags)
    }

    pub fn leak(self) -> NonNull<[u8]> {
        let inner = self.ptr;
        mem::forget(self);
        inner
    }
}

impl Drop for Virt {
    fn drop(&mut self) {
        unsafe {
            let _ = self.space.deallocate(self.base().as_non_null().unwrap());
        }
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct KernelVirt(Virt);

// [`KernelVirt`] lives in the kernel space and should share its data.
unsafe impl Send for KernelVirt {}
unsafe impl Sync for KernelVirt {}

impl KernelVirt {
    pub(super) fn new(virt: Virt) -> Result<Self, Virt> {
        match virt.ty {
            Type::Kernel => Ok(KernelVirt(virt)),
            Type::User => Err(virt),
        }
    }
}

impl Deref for KernelVirt {
    type Target = Virt;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
