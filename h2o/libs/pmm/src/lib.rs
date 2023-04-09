#![no_std]

pub mod boot;
mod buddy;

pub use paging::{LAddr, PAddr, PAGE_SHIFT, PAGE_SIZE};

#[cfg(debug_assertions)]
pub use self::buddy::dump_data;
pub use self::buddy::{
    alloc_pages, alloc_pages_exact, dealloc_pages, dealloc_pages_exact, PfType, MAX_ORDER,
    NR_ORDERS, ORDERS, PF_SIZE,
};

pub const KMEM_PHYS_BASE: usize = 0xFFFF_9000_0000_0000;

/// # Returns
///
/// `(usize, usize) => (sum, max)`.
#[inline]
pub fn init(
    mmap: &iter_ex::PtrIter<boot::MemRange>,
    reserved_range: core::ops::Range<usize>,
) -> (usize, usize) {
    buddy::init(mmap, reserved_range)
}
