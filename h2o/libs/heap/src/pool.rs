//! # The memory pool for kernel heap
//!
//! This module deals with memory pools. See [`Pool`] for more.

use core::{alloc::Layout, ptr::NonNull};

use array_macro::array;
use paging::LAddr;

use super::{alloc::Error, page::*, slab::*, stat::Stat};

/// The memory pool structure.
pub struct Pool {
    /// All the slab lists.
    slabs: [Slab; NR_OBJ_SIZES],
    stat: Stat,
}

impl Pool {
    /// Construct a new memory pool.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Pool {
        Pool {
            slabs: array![_ => Slab::new(); NR_OBJ_SIZES],
            stat: Stat::new(),
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
    /// If the memory layout doesn't match all the available [`OBJ_SIZES`],
    /// it'll return an error.
    pub fn extend(&mut self, layout: Layout, mut page: NonNull<Page>) -> Result<(), Error> {
        let idx = unwrap_layout(layout)?;
        unsafe { page.as_mut() }.init(OBJ_SIZES[idx]);
        self.slabs[idx].extend(page);

        self.stat.extend(paging::PAGE_SIZE);
        Ok(())
    }

    /// Allocate an object from the slab lists
    ///
    /// # Errors
    ///
    /// The function will return an error only if any of the following
    /// conditions are met:
    ///
    /// 1. The memory layout doesn't match all the available [`OBJ_SIZES`].
    /// 2. There's no free slab page.
    pub fn allocate(&mut self, layout: Layout) -> Result<LAddr, Error> {
        let idx = unwrap_layout(layout)?;
        self.slabs[idx]
            .pop()
            .inspect(|_| self.stat.allocate(layout.pad_to_align().size()))
    }

    /// Deallocate an object to the slab lists
    ///
    /// # Errors
    ///
    /// The function will return an error only if any of the following
    /// conditions are met:
    ///
    /// 1. The memory layout doesn't match all the available [`OBJ_SIZES`].
    /// 2. There's an internal logic fault.
    pub fn deallocate(
        &mut self,
        addr: LAddr,
        layout: Layout,
    ) -> Result<Option<NonNull<Page>>, Error> {
        let idx = unwrap_layout(layout)?;
        self.slabs[idx]
            .push(addr)
            .inspect(|_| self.stat.deallocate(layout.pad_to_align().size()))
    }

    pub fn stat(&self) -> Stat {
        self.stat.clone()
    }
}

/// Get the corresponding slab list index for the requested `Layout`.
///
/// # Errors
///
/// If the memory layout doesn't match all the available [`OBJ_SIZES`],
/// it'll return an error.
pub fn unwrap_layout(layout: Layout) -> Result<usize, Error> {
    if layout.size() == 0 {
        return Err(Error::InvLayout(layout));
    }

    let size = layout.pad_to_align().size();
    let idx = match OBJ_SIZES.binary_search(&size) {
        Ok(idx) => idx,
        Err(idx) => idx,
    };

    if !(0..NR_OBJ_SIZES).contains(&idx) {
        Err(Error::InvLayout(layout))
    } else {
        Ok(idx)
    }
}
