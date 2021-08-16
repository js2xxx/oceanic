//! # The memory pool for kernel heap
//!
//! This module deals with memory pools. See [`Pool`] for more.

use super::alloc::AllocError;
use super::page::*;
use super::slab::*;
use paging::LAddr;

use array_macro::array;
use core::alloc::Layout;
use core::ptr::NonNull;

/// The memory pool structure.
pub struct Pool {
      /// All the slab lists.
      slabs: [Slab; NR_OBJ_SIZES],
}

impl Pool {
      /// Construct a new memory pool.
      #[allow(clippy::new_without_default)]
      pub const fn new() -> Pool {
            Pool {
                  slabs: array![_ => Slab::new(); 36],
            }
      }

      /// Get the corresponding slab list index for the requested `Layout`.
      ///
      /// # Errors
      ///
      /// If the memory layout doesn't match all the available [`OBJ_SIZES`], it'll return an
      /// error.
      fn unwrap_layout(layout: Layout) -> Result<usize, AllocError> {
            if layout.size() == 0 {
                  return Err(AllocError::InvLayout(layout));
            }

            let size = layout.pad_to_align().size();
            let idx = match OBJ_SIZES.binary_search(&size) {
                  Ok(idx) => idx,
                  Err(idx) => idx,
            };

            if !(0..NR_OBJ_SIZES).contains(&idx) {
                  Err(AllocError::InvLayout(layout))
            } else {
                  Ok(idx)
            }
      }

      /// Extend a slab list with a new page
      ///
      /// # Arguments
      ///
      /// * `layout` - The object parameter that the new page fit in
      /// * `page` - The new page to be arranged
      ///
      /// # Errors
      ///
      /// If the memory layout doesn't match all the available [`OBJ_SIZES`], it'll return an
      /// error.
      pub fn extend(&mut self, layout: Layout, mut page: NonNull<Page>) -> Result<(), AllocError> {
            let idx = Self::unwrap_layout(layout)?;
            unsafe { page.as_mut() }.init(OBJ_SIZES[idx]);
            self.slabs[idx].extend(page);
            Ok(())
      }

      /// Allocate an object from the slab lists
      ///
      /// # Errors
      ///
      /// The function will return an error only if any of the following conditions are met:
      ///
      /// 1. The memory layout doesn't match all the available [`OBJ_SIZES`].
      /// 2. There's no free slab page.
      pub fn alloc(&mut self, layout: Layout) -> Result<LAddr, AllocError> {
            let idx = Self::unwrap_layout(layout)?;
            self.slabs[idx].pop()
      }

      /// Deallocate an object to the slab lists
      ///
      /// # Errors
      ///
      /// The function will return an error only if any of the following conditions are met:
      ///
      /// 1. The memory layout doesn't match all the available [`OBJ_SIZES`].
      /// 2. There's an internal logic fault.
      pub fn dealloc(
            &mut self,
            addr: LAddr,
            layout: Layout,
      ) -> Result<Option<NonNull<Page>>, AllocError> {
            let idx = Self::unwrap_layout(layout)?;
            self.slabs[idx].push(addr)
      }
}
