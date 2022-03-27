use core::{
    alloc::Layout,
    ptr::{self, NonNull},
};

use sv_call::{
    mem::{Flags, VirtMapInfo},
    Handle, Result,
};

use super::{Phys, PAGE_SIZE};
use crate::obj::Object;

/// Note: The `Virt` object's lifetime is bound to the hierarchical
/// structure instead of the handle itself (like an [`alloc::sync::Weak`]),
/// so every operation to the object can fail due to its parent destroying all
/// the nodes in the sub-tree, returning [`crate::error::Error::EKILLED`].
#[derive(Debug)]
#[repr(transparent)]
pub struct Virt(Handle);
crate::impl_obj!(Virt);
crate::impl_obj!(@DROP, Virt);

impl Virt {
    pub fn allocate(&self, offset: Option<usize>, layout: Layout) -> Result<Self> {
        let handle = unsafe {
            sv_call::sv_virt_alloc(
                // SAFETY: We don't move the ownership of the handle.
                unsafe { self.raw() },
                offset.unwrap_or(usize::MAX),
                layout.size(),
                layout.align(),
            )
        }
        .into_res()?;
        // SAFETY: The handle is freshly allocated.
        Ok(unsafe { Self::from_raw(handle) })
    }

    pub fn try_get_base(&self) -> Result<NonNull<u8>> {
        // SAFETY: We don't move the ownership of the handle.
        let value =
            unsafe { sv_call::sv_virt_info(unsafe { self.raw() }, ptr::null_mut()) }.into_res()?;
        Ok(unsafe { NonNull::new_unchecked(value as *mut u8) })
    }

    pub fn base(&self) -> NonNull<u8> {
        self.try_get_base()
            .expect("Failed to get the base of the virt")
    }

    /// # Safety
    ///
    /// The caller must ensure that `size < usize::MAX - PAGE_SIZE - 1`.
    pub unsafe fn page_aligned(size: usize) -> Layout {
        Layout::from_size_align_unchecked(size, PAGE_SIZE)
    }

    pub fn map(
        &self,
        offset: Option<usize>,
        phys: Phys,
        phys_offset: usize,
        layout: Layout,
        flags: Flags,
    ) -> Result<NonNull<[u8]>> {
        let layout = layout.pad_to_align();
        let mi = VirtMapInfo {
            offset: offset.unwrap_or(usize::MAX),
            phys: Phys::into_raw(phys),
            phys_offset,
            len: layout.size(),
            align: layout.align(),
            flags,
        };
        // SAFETY: We don't move the ownership of the handle.
        let value = unsafe { sv_call::sv_virt_map(unsafe { self.raw() }, &mi) }.into_res()?;
        // SAFETY: The pointer range is freshly allocated.
        Ok(unsafe {
            let ptr = NonNull::new_unchecked(value as *mut u8);
            NonNull::slice_from_raw_parts(ptr, layout.size())
        })
    }

    pub fn map_phys(
        &self,
        offset: Option<usize>,
        phys: Phys,
        flags: Flags,
    ) -> Result<NonNull<[u8]>> {
        let len = phys.len();
        self.map(offset, phys, 0, unsafe { Self::page_aligned(len) }, flags)
    }

    pub fn map_vdso(&self, vdso: Phys) -> Result<NonNull<[u8]>> {
        let len = vdso.len();
        self.map(
            None,
            vdso,
            0,
            unsafe { Self::page_aligned(len) },
            Flags::READABLE | Flags::EXECUTABLE | Flags::USER_ACCESS,
        )
    }

    pub fn reprotect(&self, base: NonNull<u8>, len: usize, flags: Flags) -> Result {
        // SAFETY: We don't move the ownership of the handle.
        unsafe { sv_call::sv_virt_reprot(unsafe { self.raw() }, base.as_ptr(), len, flags) }
            .into_res()
    }

    pub fn unmap(&self, base: NonNull<u8>, len: usize, drop_child: bool) -> Result {
        // SAFETY: We don't move the ownership of the handle.
        unsafe { sv_call::sv_virt_unmap(unsafe { self.raw() }, base.as_ptr(), len, drop_child) }
            .into_res()
    }

    /// Implicitly dropping the handle will not affect the hierarchical
    /// structure of `Virt`s.
    pub fn destroy(self) -> Result {
        unsafe { sv_call::sv_virt_drop(Self::into_raw(self)) }.into_res()
    }
}
