bitflags::bitflags! {
    /// Flags to describe a block of memory.
    pub struct Flags: u32 {
        const USER_ACCESS = 1;
        const READABLE    = 1 << 1;
        const WRITABLE    = 1 << 2;
        const EXECUTABLE  = 1 << 3;
        const UNCACHED    = 1 << 4;
        const ZEROED      = 1 << 10;
    }
}

cfg_if::cfg_if! { if #[cfg(feature = "call")] {

use core::{alloc::Layout, ptr::NonNull};

pub fn virt_alloc(
    virt: &mut *mut u8,
    phys: usize,
    layout: Layout,
    flags: Flags,
) -> crate::Result<crate::Handle> {
    let (size, align) = (layout.size(), layout.align());
    let flags = flags.bits;
    crate::call::virt_alloc(virt, phys, size, align, flags)
}

/// # Safety
///
/// The caller must ensure that `ptr` is only in the possession of current
/// context.
pub unsafe fn virt_protect(
    hdl: crate::Handle,
    ptr: NonNull<[u8]>,
    flags: Flags,
) -> crate::Result<()> {
    let size = ptr.len();
    crate::call::virt_prot(hdl, ptr.as_mut_ptr(), size, flags.bits)
}

pub fn mem_alloc(layout: Layout, flags: Flags) -> crate::Result<NonNull<[u8]>> {
    let (size, align) = (layout.size(), layout.align());
    let ptr = crate::call::mem_alloc(size, align, flags.bits)?;
    unsafe {
        Ok(NonNull::slice_from_raw_parts(
            NonNull::new_unchecked(ptr),
            size,
        ))
    }
}

pub unsafe fn mem_dealloc(ptr: NonNull<u8>) -> crate::Result<()> {
    crate::call::mem_dealloc(ptr.as_ptr())
}

}}
