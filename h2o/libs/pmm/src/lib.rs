#![no_std]
#![feature(asm)]
#![feature(nonnull_slice_from_raw_parts)]

pub mod buddy;

#[cfg(debug_assertions)]
pub use buddy::dump_data;
pub use buddy::{
    alloc_pages, alloc_pages_exact, dealloc_pages, dealloc_pages_exact, PfType, MAX_ORDER,
    NR_ORDERS, ORDERS, PF_SIZE,
};
pub use paging::{LAddr, PAddr, PAGE_SHIFT, PAGE_SIZE};

pub const KMEM_PHYS_BASE: usize = 0xFFFF_9000_0000_0000;

#[inline]
pub fn init(
    efi_mmap_paddr: paging::PAddr,
    efi_mmap_len: usize,
    efi_mmap_unit: usize,
    reserved_range: core::ops::Range<usize>,
) -> usize {
    let efi_mmap_ptr = *efi_mmap_paddr as *mut uefi::table::boot::MemoryDescriptor;
    buddy::init(efi_mmap_ptr, efi_mmap_len, efi_mmap_unit, reserved_range)
}
