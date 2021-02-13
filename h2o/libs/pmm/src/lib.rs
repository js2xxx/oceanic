#![no_std]
#![feature(asm)]
#![feature(int_bits_const)]

use core::ptr::NonNull;

pub mod buddy;

pub use paging::{LAddr, PAddr, PAGE_SIZE, PAGE_SHIFT};

pub use buddy::PF_SIZE;

pub use buddy::MAX_ORDER;
pub use buddy::NR_ORDERS;
pub use buddy::ORDERS;

pub use buddy::PFType;

pub use buddy::alloc_pages;
pub use buddy::alloc_pages_exact;
pub use buddy::dealloc_pages;
pub use buddy::dealloc_pages_exact;
// #[cfg(debug_assertions)]
// pub use buddy::dump_data;

pub const KMEM_PHYS_BASE: usize = 0xFFFF_9000_0000_0000;

#[inline]
pub fn init(mmap: NonNull<[uefi::table::boot::MemoryDescriptor]>) {
      buddy::init(mmap);
}