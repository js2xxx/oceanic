#![no_std]
#![feature(asm)]
#![feature(nonnull_slice_from_raw_parts)]

pub mod buddy;

pub use paging::{LAddr, PAddr, PAGE_SHIFT, PAGE_SIZE};

pub use buddy::PF_SIZE;

pub use buddy::MAX_ORDER;
pub use buddy::NR_ORDERS;
pub use buddy::ORDERS;

pub use buddy::PfType;

pub use buddy::alloc_pages;
pub use buddy::alloc_pages_exact;
pub use buddy::dealloc_pages;
pub use buddy::dealloc_pages_exact;
#[cfg(debug_assertions)]
pub use buddy::dump_data;

pub const KMEM_PHYS_BASE: usize = 0xFFFF_9000_0000_0000;

#[inline]
pub fn init(
      efi_mmap_paddr: paging::PAddr,
      efi_mmap_len: usize,
      efi_mmap_unit: usize,
      reserved_range: core::ops::Range<usize>,
) {
      let efi_mmap_ptr = *efi_mmap_paddr as *mut uefi::table::boot::MemoryDescriptor;
      buddy::init(efi_mmap_ptr, efi_mmap_len, efi_mmap_unit, reserved_range);
}
