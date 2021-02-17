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
//! Each slab page is a headered list sized [`PAGE_SIZE`], whose header contains a bitmap
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
//! A memory pool holds slab lists of different object sizes (see [`OBJ_SIZES`]). When requested,
//! It looks for a correct index for the requested size and hands the task to the slab list of
//! that idx.
//!
//! See module [`pool`] for more.
//!
//! ### Global allocator implementation
//!
//! The global allocator links to the Rust [`alloc`] library for advanced use. It delegates its
//! memory pool or directly page allocator on request.
//!
//! See [`DefaultAlloc`] for more.

#![no_std]
#![feature(const_fn_fn_ptr_basics)]
#![feature(lang_items)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(slice_ptr_get)]

pub mod alloc;
pub mod page;
pub mod pool;
pub mod slab;

pub use page::{Page, AllocPages, DeallocPages};

use core::ptr::NonNull;

#[inline(never)]
fn null_alloc_pages(_n: usize) -> Option<NonNull<[page::Page]>> {
      None
}

#[inline(never)]
fn null_dealloc_pages(_pages: NonNull<[page::Page]>) {}

#[global_allocator]
static GLOBAL_ALLOC: alloc::DefaultAlloc = alloc::DefaultAlloc {
      pool: spin::Mutex::new(unsafe { core::mem::transmute(pool::Pool::new()) }),
      pager: spin::Mutex::new(page::Pager::new(null_alloc_pages, null_dealloc_pages)),
};

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
