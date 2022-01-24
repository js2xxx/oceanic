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

pub struct MapInfo {
    pub addr: usize,
    pub map_addr: bool,
    pub phys: crate::Handle,
    pub phys_offset: usize,
    pub len: usize,
    pub flags: Flags,
}

cfg_if::cfg_if! { if #[cfg(feature = "call")] {

use core::{alloc::Layout, ptr::NonNull};

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

/// # Safety
///
/// The caller must ensure that `ptr` is previously allocated by [`mem_alloc`].
pub unsafe fn mem_dealloc(ptr: NonNull<u8>) -> crate::Result<()> {
    crate::call::mem_unmap(ptr.as_ptr())
}

}}

#[cfg(feature = "call")]
pub fn test() {
    let flags = Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS;
    let phys = crate::call::phys_alloc(4096, 4096, flags.bits)
        .expect("Failed to allocate physical object");
    let mi = MapInfo {
        addr: 0,
        map_addr: false,
        phys,
        phys_offset: 0,
        len: 4096,
        flags,
    };
    let ptr = crate::call::mem_map(&mi).expect("Failed to map the physical memory");
    unsafe { ptr.cast::<u32>().write(12345) };
    crate::call::mem_unmap(ptr).expect("Failed to unmap the memory");
    crate::call::obj_drop(phys).expect("Failed to deallocate the physical object");
}
