use super::page::Pager;
use super::{page, pool};
use bitop_ex::BitOpEx;
use paging::LAddr;

use core::alloc::{AllocError, Allocator as AllocTrait, GlobalAlloc, Layout};
use core::ptr::{null_mut, NonNull};
use spin::Mutex;

/// The kinds of allocation errors.
#[derive(Debug)]
pub enum Error {
      /// Internal error, including an description.
      ///
      /// Should not be exported to outer error handlers.
      Internal(&'static str),

      /// Need extending.
      ///
      /// If no slab page is free, the function returns this for the superior to call
      /// `extend`. Should not be exported to outer error handlers.
      NeedExt,

      /// Invalid memory layout.
      ///
      /// Indicates that the conditions of the requested `Layout` cannot be satisfied.
      InvLayout(Layout),
}

/// The the default global allocator type of Rust library.
///
/// All the members inside are encapsulated in [`Mutex`] because its functions are to be
/// called globally (a.k.a. multi-CPU) so they must be locked to prevent data race.
pub struct Allocator {
      /// The main memory pool
      pub(super) pool: Mutex<pool::Pool>,
      /// The default pager
      pub(super) pager: Mutex<Pager>,
}

impl Allocator {
      pub const fn new(alloc_pages: crate::AllocPages, dealloc_pages: crate::DeallocPages) -> Self {
            Allocator {
                  pool: Mutex::new(pool::Pool::new()),
                  pager: Mutex::new(Pager::new(alloc_pages, dealloc_pages)),
            }
      }

      pub const fn new_null() -> Self {
            Self::new(null_alloc_pages, null_dealloc_pages)
      }

      pub fn stat(&self) -> crate::stat::Stat {
            self.pool.lock().stat()
      }

      pub unsafe fn set_alloc(
            &self,
            alloc_pages: crate::AllocPages,
            dealloc_pages: crate::DeallocPages,
      ) {
            let mut pager = self.pager.lock();
            *pager = page::Pager::new(alloc_pages, dealloc_pages);
      }

      pub unsafe fn reset_alloc(&self) {
            let mut pager = self.pager.lock();
            *pager = page::Pager::new(null_alloc_pages, null_dealloc_pages);
      }
}

unsafe impl Sync for Allocator {}

unsafe impl GlobalAlloc for Allocator {
      unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // The actual size calculation
            let size = layout.pad_to_align().size();

            // The size is not too big
            if size <= page::MAX_OBJ_SIZE {
                  let mut pool = self.pool.lock();

                  // The first allocation (assuming something available)
                  match pool.alloc(layout).map(|x| *x) {
                        // Whoosh! Returning
                        Ok(x) => x,

                        Err(e) => match e {
                              // Oops! The pool is full
                              Error::NeedExt => {
                                    let page = {
                                          let mut pager = self.pager.lock();
                                          // Allocate a new page
                                          pager.alloc_pages(1)
                                    };

                                    if let Some(page) = page {
                                          pool.extend(layout, page.cast()).unwrap();
                                          // The second allocation
                                          pool.alloc(layout).map_or(null_mut(), |x| *x)
                                    } else {
                                          // A-o! Out of memory
                                          null_mut()
                                    }
                              }
                              // A-o! There's a bug
                              _ => null_mut(),
                        },
                  }
            } else {
                  // The size is too big, call the pager directly
                  let n = size.div_ceil_bit(paging::PAGE_SHIFT);
                  let mut pager = self.pager.lock();
                  pager.alloc_pages(n)
                        .map_or(null_mut(), |x| x.as_ptr().cast())
            }
      }

      unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // The actual size calculation
            let size = layout.pad_to_align().size();

            // The size is not too big
            if size <= page::MAX_OBJ_SIZE {
                  let mut pool = self.pool.lock();

                  // Deallocate it
                  if let Some(page) = pool.dealloc(LAddr::new(ptr), layout).unwrap_or(None) {
                        // A page is totally empty, drop it
                        let mut pager = self.pager.lock();
                        pager.dealloc_pages(NonNull::slice_from_raw_parts(page, 1));
                  }
            } else {
                  // The size is too big, call the pager directly
                  let n = size.div_ceil_bit(paging::PAGE_SHIFT);
                  let page = NonNull::new(ptr.cast::<page::Page>()).expect("Null pointer provided");
                  let mut pager = self.pager.lock();
                  pager.dealloc_pages(NonNull::slice_from_raw_parts(page, n));
            }
      }
}

unsafe impl AllocTrait for Allocator {
      fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
            let size = layout.size();
            let ptr = unsafe { (self as &dyn GlobalAlloc).alloc(layout) };
            NonNull::new(ptr)
                  .map(|ptr| NonNull::slice_from_raw_parts(ptr, size))
                  .ok_or(AllocError)
      }

      unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
            (self as &dyn GlobalAlloc).dealloc(ptr.as_ptr(), layout)
      }
}

#[inline(never)]
fn null_alloc_pages(_n: usize) -> Option<NonNull<[page::Page]>> {
      None
}

#[inline(never)]
fn null_dealloc_pages(_pages: NonNull<[page::Page]>) {}
