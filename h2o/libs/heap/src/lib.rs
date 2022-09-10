//! # Oceanic's kernel heap module
//!
//! This module mainly deals with usual memory allocations for use inside the
//! kernel. We want readily accessible memory blocks of small sizes so we use a
//! flexible allocator for this purpose.
//!
//! ## The global allocator
//!
//! The global allocator imitates the ideas of Linux's slab allocator. It caches
//! multiple slabs of different sizes and allocates them if requested.
//!
//! ### Slab pages
//!
//! Each slab page is a headered list sized [`paging::PAGE_SIZE`], whose header
//! contains a bitmap recording the uses of items of the list. When an item is
//! allocated (popped), the corresponding bit will be set `true`.
//!
//! See module [`page`] for more.
//!
//! ### Slab lists
//!
//! It'll usually not enough for massive allocations for the same size to only
//! one slab page, so there're lists (implemented with red-black trees) to hold
//! slab pages with the same object size. On request, the list picks a free
//! (both partially and wholly) slab page and hands the task over.
//!
//! See module [`slab`] for more.
//!
//! ### Memory pools
//!
//! A memory pool holds slab lists of different object sizes (see
//! [`page::OBJ_SIZES`]). When requested, it looks for a correct index for the
//! requested size and hands the task to the slab list of that idx.
//!
//! See module [`pool`] for more.
//!
//! ### Global allocator implementation
//!
//! The global allocator links to the Rust [`::alloc`] library for advanced use.
//! It delegates its memory pool or directly page allocator on request.
//!
//! See [`alloc::Allocator`] for more.

#![no_std]
#![feature(allocator_api)]
#![feature(lang_items)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(result_option_inspect)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]
#![feature(thread_local)]

mod alloc;
mod page;
mod stat;

mod pool;
mod slab;
mod tcache;

#[cfg(not(feature = "tcache"))]
pub use self::alloc::Allocator;
pub use self::{
    page::{AllocPages, DeallocPages, Page, MAX_OBJ_SIZE, OBJ_SIZES},
    pool::unwrap_layout,
    stat::Stat,
    tcache::ThreadCache,
};

#[cfg(feature = "tcache")]
#[thread_local]
static mut TCACHE: tcache::ThreadCache = tcache::ThreadCache::new();

cfg_if::cfg_if! { if #[cfg(feature = "global")] {

#[global_allocator]
static GLOBAL_ALLOC: alloc::Allocator = alloc::Allocator::new_null();

/// Set the functions for allocating and deallocating pages.
pub unsafe fn set_alloc(alloc_pages: page::AllocPages, dealloc_pages: page::DeallocPages) {
    GLOBAL_ALLOC.set_alloc(alloc_pages, dealloc_pages)
}

/// Reset the function for allocating and deallocating pages.
pub unsafe fn reset_alloc() {
    GLOBAL_ALLOC.reset_alloc()
}

pub fn stat() -> stat::Stat {
    GLOBAL_ALLOC.stat()
}

#[inline]
pub fn test_global(start_seed: usize) {
    test(&GLOBAL_ALLOC, start_seed);
}

}}

/// The test function for the module.
#[allow(dead_code)]
#[allow(unused_variables)]
pub fn test(a: &impl core::alloc::GlobalAlloc, start_seed: usize) {
    #[cfg(debug_assertions)]
    {
        use core::alloc::Layout;
        fn random(mut seed: usize) -> usize {
            let mut ret = ((seed % 0x100001).pow(3) >> 6) & paging::PAGE_MASK;
            while ret == 0 {
                seed += 2;
                ret = ((seed % 0xFA53DCEB).pow(2) >> 5) & paging::PAGE_MASK
            }
            ret
        }

        let mut seed = random(start_seed);
        let mut k = [(core::ptr::null_mut(), Layout::for_value(&0)); 100];
        let allocator = a;

        let n1 = k.len() / 3 + seed % (k.len() / 3);
        let n2 = k.len() - n1;

        // Safety: All the allocations are paired and thus legal.
        unsafe {
            for u in k.iter_mut().take(n2) {
                let layout = core::alloc::Layout::from_size_align(seed, seed.next_power_of_two())
                    .expect("Invalid layout");
                *u = (allocator.alloc(layout), layout);
                seed = random(seed);
            }

            for v in k.iter().take(n1) {
                allocator.dealloc(v.0, v.1);
            }

            for w in k.iter_mut().skip(n2) {
                let layout = core::alloc::Layout::from_size_align(seed, seed.next_power_of_two())
                    .expect("Invalid layout");
                *w = (allocator.alloc(layout), layout);
                seed = random(seed);
            }

            for a in k.iter().skip(n1) {
                allocator.dealloc(a.0, a.1);
            }
        }
    }
}
