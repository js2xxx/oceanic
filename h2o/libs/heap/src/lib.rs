//! # Oceanic's kernel heap module
//!
//! This module mainly deals with usual memory allocations for use inside the kernel. We want
//! readily accessible memory blocks of small sizes so we use a flexible allocator for this
//! purpose.
//!
//! ## The global allocator
//!
//! The global allocator imitates the ideas of Linux's slab allocator. It caches multiple slabs
//! of different sizes and allocates them if requested.
//!
//! ### Slab pages
//!
//! Each slab page is a headered list sized [`paging::PAGE_SIZE`], whose header contains a bitmap
//! recording the uses of items of the list. When an item is allocated (popped), the
//! corresponding bit will be set `true`.
//!
//! See module [`page`] for more.
//!
//! ### Slab lists
//!
//! It'll usually not enough for massive allocations for the same size to only one slab page,
//! so there're lists (implemented with red-black trees) to hold slab pages with the same
//! object size. On request, the list picks a free (both partially and wholly) slab page and
//! hands the task over.
//!
//! See module [`slab`] for more.
//!
//! ### Memory pools
//!
//! A memory pool holds slab lists of different object sizes (see [`page::OBJ_SIZES`]). When
//! requested, it looks for a correct index for the requested size and hands the task to the
//! slab list of that idx.
//!
//! See module [`pool`] for more.
//!
//! ### Global allocator implementation
//!
//! The global allocator links to the Rust [`alloc`] library for advanced use. It delegates its
//! memory pool or directly page allocator on request.
//!
//! See [`alloc::DefaultAlloc`] for more.

#![no_std]
#![feature(const_fn_fn_ptr_basics)]
#![feature(lang_items)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(result_into_ok_or_err)]
#![feature(slice_ptr_get)]
#![feature(ptr_internals)]

pub mod alloc;
pub mod page;
pub mod pool;
pub mod slab;
pub mod stat;

pub use page::{AllocPages, DeallocPages, Page};

use core::ptr::Unique;

#[global_allocator]
static GLOBAL_ALLOC: alloc::DefaultAlloc = alloc::DefaultAlloc {
      pool: spin::Mutex::new(pool::Pool::new()),
      pager: spin::Mutex::new(page::Pager::new(null_alloc_pages, null_dealloc_pages)),
};

#[inline(never)]
fn null_alloc_pages(_n: usize) -> Option<Unique<[page::Page]>> {
      None
}

#[inline(never)]
fn null_dealloc_pages(_pages: Unique<[page::Page]>) {}

/// Set the functions for allocating and deallocating pages.
pub fn set_alloc(alloc_pages: AllocPages, dealloc_pages: DeallocPages) {
      let mut pager = GLOBAL_ALLOC.pager.lock();
      *pager = page::Pager::new(alloc_pages, dealloc_pages);
}

/// Reset the function for allocating and deallocating pages.
pub fn reset_alloc() {
      let mut pager = GLOBAL_ALLOC.pager.lock();
      *pager = page::Pager::new(null_alloc_pages, null_dealloc_pages);
}

pub fn stat() -> stat::Stat {
      GLOBAL_ALLOC.pool.lock().stat()
}

/// The test function for the module.
pub fn test() {
      if !cfg!(debug_assertions) {
            return;
      }

      use core::alloc::Layout;
      fn random(seed: usize) -> usize {
            (2357 * seed + 7631) >> paging::PAGE_SHIFT
      }

      let mut seed = 324;
      let mut k = [(core::ptr::null_mut(), Layout::for_value(&0)); 100];
      let allocator = &GLOBAL_ALLOC as &dyn core::alloc::GlobalAlloc;

      let n1 = 36;
      let n2 = 78;

      // Safety: All the allocations are paired and thus legal.
      unsafe {
            for u in k.iter_mut().take(n2) {
                  let layout = core::alloc::Layout::from_size_align(seed, seed.next_power_of_two())
                        .expect("Invalid layout");
                  *u = (allocator.alloc(layout), layout);
                  seed = random(seed);
            }

            for v in k.iter_mut().take(n1) {
                  allocator.dealloc(v.0, v.1);
            }

            for w in k.iter_mut().skip(n2) {
                  let layout = core::alloc::Layout::from_size_align(seed, seed.next_power_of_two())
                        .expect("Invalid layout");
                  *w = (allocator.alloc(layout), layout);
                  seed = random(seed);
            }

            for a in k.iter_mut().skip(n1) {
                  allocator.dealloc(a.0, a.1);
            }
      }
}
