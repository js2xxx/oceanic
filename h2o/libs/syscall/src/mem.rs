use core::alloc::Layout;

bitflags::bitflags! {
    /// Flags to describe a block of memory.
    pub struct Flags: u32 {
        const USER_ACCESS = 1;
        const READABLE    = 1 << 1;
        const WRITABLE    = 1 << 2;
        const EXECUTABLE  = 1 << 3;
        const ZEROED      = 1 << 4;
    }
}

pub fn alloc_pages(
    virt: *mut u8,
    phys: usize,
    layout: Layout,
    flags: Flags,
) -> crate::Result<*mut [u8]> {
    let (size, align) = (layout.size(), layout.align());
    let flags = flags.bits;
    crate::call::alloc_pages(virt, phys, size, align, flags)
        .map(|ptr| unsafe { core::slice::from_raw_parts_mut(ptr, size) as *mut _ })
}

pub fn dealloc_pages(ptr: *mut u8) -> crate::Result<usize> {
    crate::call::dealloc_pages(ptr)
}

/// # Safety
///
/// The caller must ensure that `ptr` is only in the possession of current
/// context.
pub unsafe fn modify_pages(ptr: *mut [u8], flags: Flags) -> crate::Result<()> {
    let size = ptr.len();
    crate::call::modify_pages(ptr.as_mut_ptr(), size, flags.bits)
}