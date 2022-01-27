#![no_std]
#![feature(nonnull_slice_from_raw_parts)]

pub mod boot;
mod buddy;

#[cfg(debug_assertions)]
pub use buddy::dump_data;
pub use buddy::{
    alloc_pages, alloc_pages_exact, dealloc_pages, dealloc_pages_exact, PfType, MAX_ORDER,
    NR_ORDERS, ORDERS, PF_SIZE,
};
pub use paging::{LAddr, PAddr, PAGE_SHIFT, PAGE_SIZE};

pub const KMEM_PHYS_BASE: usize = 0xFFFF_9000_0000_0000;

/// # Returns
///
/// `(usize, usize) => (sum, max)`.
#[inline]
pub fn init(
    mmap: &iter_ex::PointerIterator<boot::MemRange>,
    reserved_range: core::ops::Range<usize>,
) -> (usize, usize) {
    buddy::init(mmap, reserved_range)
}
