//! # The slab list module
//!
//! This module deals with slab lists in kernel heap. See [`Slab`] for more.

mod adapter;

use super::alloc::AllocError;
use super::page::*;
use adapter::PageAdapter;
use paging::{LAddr, PAGE_MASK};

use core::ptr::NonNull;
use intrusive_collections::{Bound, RBTree};
use spin::Mutex;

/// The slab list structure.
///
/// Slab lists use red-black tree to index slab pages for accurate picking operations.
#[derive(Default)]
pub struct Slab {
      /// The inner red-black tree.
      list: RBTree<PageAdapter>,
      /// The mutex lock.
      lock: Mutex<()>,
}

impl Slab {
      /// Construct a new slab list.
      pub const fn new() -> Self {
            Self {
                  list: RBTree::new(PageAdapter::NEW),
                  lock: Mutex::new(()),
            }
      }

      /// Extend the slab list with a new slab page.
      ///
      /// This will gain more allocation space, thus making it able to return available
      /// objects
      pub fn extend(&mut self, page: NonNull<Page>) {
            let _lock = self.lock.lock();
            self.list.insert(page);
      }

      /// Pop an object from the slab list
      ///
      /// This function takes a partially free slab page with the lowest free count, which
      /// let one slab page be filled first and then next one, optimizing CPU TLB access.
      /// Then it gets an object from that page and returns.
      ///
      /// # Errors
      ///
      /// If no slab page is free, it'll return an error in need of new pages.
      pub fn pop(&mut self) -> Result<LAddr, AllocError> {
            let _lock = self.lock.lock();

            let mut front = self.list.lower_bound_mut(Bound::Excluded(&0));
            if let Some(mut ptr) = front.remove() {
                  let ret = {
                        let page = unsafe { ptr.as_mut() };
                        page.pop()
                  }?;
                  self.list.insert(ptr);
                  Ok(ret)
            } else {
                  Err(AllocError::NeedExt)
            }
      }

      /// Push an object to the slab list
      ///
      /// This function calculates the corresponding slab page and let it take the object, and
      /// Then the page is removed from the list. If the page is totally free, then it'll be
      /// recycled (returned), or else pushed back to the list.
      ///
      /// # Errors
      ///
      /// If the page is not valid or something else (see `Page::push` for more), it'll return
      /// an error.
      pub fn push(&mut self, addr: LAddr) -> Result<Option<NonNull<Page>>, AllocError> {
            let _lock = self.lock.lock();

            let mut base = NonNull::new((addr.val() & !PAGE_MASK) as *mut Page)
                  .map_or(Err(AllocError::Internal("Null address")), Ok)?;

            let partially_free = {
                  let page = unsafe { base.as_mut() };
                  page.push(addr)?;
                  page.used_count() < page.max_count()
            };

            let mut cur = unsafe { self.list.cursor_mut_from_ptr(base.as_ptr()) };
            cur.remove();
            if partially_free {
                  self.list.insert(base);
                  Ok(None)
            } else {
                  Ok(Some(base))
            }
      }
}
