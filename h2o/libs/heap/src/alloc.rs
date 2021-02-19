use super::page::Pager;
use super::{page, pool};
use bitop_ex::BitOpEx;
use paging::LAddr;

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{null_mut, NonNull};
use spin::Mutex;

/// The the default global allocator type of Rust library.
///
/// All the members inside are encapsulated in [`Mutex`] because its functions are to be
/// called globally (a.k.a. multi-CPU) so they must be locked to prevent data race.
pub struct DefaultAlloc {
      /// The main memory pool
      pub(super) pool: Mutex<[u8; core::mem::size_of::<pool::Pool>()]>,
      /// The default pager
      pub(super) pager: Mutex<Pager>,
}

/// The kinds of allocation errors.
#[derive(Debug)]
pub enum AllocError {
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

unsafe impl GlobalAlloc for DefaultAlloc {
      unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // The actual size calculation
            let size = layout.pad_to_align().size();

            // The size is not too big
            if size <= page::MAX_OBJ_SIZE {
                  let pool = self.pool.lock();
                  let pool = &mut *(pool.as_ptr() as *mut pool::Pool);

                  // The first allocation (assuming something available)
                  match pool.alloc(layout).map(|x| *x) {
                        // Whoosh! Returning
                        Ok(x) => x,

                        Err(e) => match e {
                              // Oops! The pool is full
                              AllocError::NeedExt => {
                                    let page = {
                                          let mut pager = self.pager.lock();
                                          // Allocate a new page
                                          pager.alloc_pages(1)
                                    };

                                    if let Some(page) = page {
                                          pool.extend(layout, page.as_non_null_ptr()).unwrap();
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
                        .map_or(null_mut(), |x| x.as_mut_ptr().cast())
            }
      }

      unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // The actual size calculation
            let size = layout.pad_to_align().size();

            // The size is not too big
            if size <= page::MAX_OBJ_SIZE {
                  let pool = self.pool.lock();
                  let pool = &mut *(pool.as_ptr() as *mut pool::Pool);

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
