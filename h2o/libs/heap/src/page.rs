//! # The slab page module
//!
//! This module deals with slab pages.
//!
//! ## Slab pages
//!
//! A normal page is just a memory block sized [`PAGE_SIZE`], while a slab page is a normal
//! page added with certain data structures stored within that memory area.
//!
//! A slab page is divided into "objects" - memory blocks whose size (defined as [`OBJ_SIZES`])
//! is relatively small. It also contains a bitmap recording the use of those so-called objects,
//! and a red-black tree link to the slab list defined in [`super::slab`].
//!
//! > NOTE: A slab page must be defined in a valid and factual (mapped to a valid physical
//! > address) page!

use super::alloc::AllocError;
use paging::{LAddr, PAGE_SIZE};

use bitvec::prelude::*;
use core::mem::{align_of, size_of};
use core::ptr::Unique;
use intrusive_collections::RBTreeLink;
use static_assertions::*;

pub type AllocPages = unsafe fn(n: usize) -> Option<Unique<[Page]>>;
pub type DeallocPages = unsafe fn(pages: Unique<[Page]>);

/// Defines the sizes of objects.
///
/// Object sizes are discrete, which simplifies allocation and alignment. They're divided
/// into 3 classes. The constants in each class are made of arithmetic and geometric series.
pub const OBJ_SIZES: [usize; 36] = [
      16, 24, // \ - Class 1
      32, 48, // /
      64, 80, 96, 112, // \ - Class 2
      128, 160, 192, 224, // |
      256, 320, 384, 448, // |
      512, 640, 768, 896, // /
      1024, 1152, 1280, 1408, 1536, 1664, 1792, 1920, // \ - Class 3
      2048, 2304, 2560, 2816, 3072, 3328, 3584, 3840, // /
];

/// The number of the items of [`OBJ_SIZES`].
pub const NR_OBJ_SIZES: usize = OBJ_SIZES.len();

/// The minimum object size.
pub const MIN_OBJ_SIZE: usize = OBJ_SIZES[0];

/// The maximum object size.
pub const MAX_OBJ_SIZE: usize = OBJ_SIZES[NR_OBJ_SIZES - 1];

/// The slab page type.
///
/// See [the module level doc](./index.html) for more.
#[repr(C, align(4096))]
pub struct Page {
      /// The link to a slab list.
      pub(crate) link: RBTreeLink,

      /// The object size of this slab page.
      objsize: usize,

      /// The bitmap records.
      used: BitArr!(for PAGE_SIZE / MIN_OBJ_SIZE),
      used_count: usize,

      /// The remaining (free) data of the slab page.
      data: [u8; PAGE_SIZE - Self::HEADER_SIZE],
}

// The size of a slab page must be [`PAGE_SIZE`].
const_assert_eq!(size_of::<Page>(), PAGE_SIZE);

// The alignment of a slab page must be [`PAGE_SIZE`].
const_assert_eq!(align_of::<Page>(), PAGE_SIZE);

impl Page {
      /// The header size of a slab page.
      ///
      /// > NOTE: if some change of fields takes place, this constant must be manually
      /// > modified!
      const HEADER_SIZE: usize = size_of::<RBTreeLink>() // self.link
             + size_of::<usize>() // self.objsize
             + size_of::<BitArr!(for PAGE_SIZE / MIN_OBJ_SIZE)>() // self.used
             + size_of::<usize>(); // self.used_count

      /// Get the count of objects that the header takes up the space of.
      fn header_count(&self) -> usize {
            Self::HEADER_SIZE / self.objsize + ((Self::HEADER_SIZE % self.objsize != 0) as usize)
      }

      /// Get the max possible count of this slab page.
      pub fn max_count(&self) -> usize {
            PAGE_SIZE / self.objsize
      }

      /// Get the count of occupied objects.
      pub fn used_count(&self) -> usize {
            self.used_count
      }

      /// Get the count of available objects.
      pub fn free_count(&self) -> usize {
            self.max_count() - self.used_count()
      }

      /// Initialize a slab page.
      ///
      /// # Arguments
      ///
      /// * `sz` - The requested object size.
      pub fn init(&mut self, sz: usize) {
            self.link = RBTreeLink::new();
            self.objsize = sz;
            self.used = BitArray::zeroed();

            let hdrcnt = self.header_count();
            for i in 0..hdrcnt {
                  self.used.set(i, true);
                  self.used_count += 1;
            }
      }

      /// Pop an object from the slab page.
      ///
      /// This function simply traverses the bitmap and looks for a bit set `false`.
      /// If something is found, the corresponding address of the object is returned.
      ///
      /// # Errors
      ///
      /// The function won't be called if the page is fully occupied in normal conditions,
      /// or it'll throw an internal error.
      pub fn pop(&mut self) -> Result<LAddr, AllocError> {
            let cnt = self.max_count();
            let hdrcnt = self.header_count();
            for i in hdrcnt..cnt {
                  if !self.used[i] {
                        self.used.set(i, true);
                        self.used_count += 1;

                        let base = LAddr::new(self as *const Page as *mut u8);
                        return Ok(LAddr::new(unsafe { base.add(self.objsize * i) }));
                  }
            }
            Err(AllocError::Internal("Fully busy but popped from the slab"))
      }

      /// Push an object to the slab page.
      ///
      /// It first validates that the requested address is within the range of the slab page.
      /// If valid, the address is transformed into the corresponding bit, which is then set
      /// false.
      ///
      /// # Errors
      ///
      /// If the address is invalid (out of range) or the object is already set free, the
      /// will return an error.
      pub fn push(&mut self, addr: LAddr) -> Result<(), AllocError> {
            let base = LAddr::new((self as *mut Page).cast());
            let idx = (addr.val() - base.val()) / self.objsize;
            if !(0..self.max_count()).contains(&idx) {
                  Err(AllocError::Internal("Address out of range"))
            } else if !self.used[idx] {
                  Err(AllocError::Internal("Address already deallocated"))
            } else {
                  self.used.set(idx, false);
                  self.used_count -= 1;
                  Ok(())
            }
      }
}

pub struct Pager {
      alloc_pages: AllocPages,
      dealloc_pages: DeallocPages,
}

impl Pager {
      pub const fn new(alloc_pages: AllocPages, dealloc_pages: DeallocPages) -> Self {
            Pager {
                  alloc_pages,
                  dealloc_pages,
            }
      }

      /// # Safety
      ///
      /// It'll always be safe **ONLY IF** it's called single-thread, and its components
      /// won't fail.
      pub unsafe fn alloc_pages(&mut self, n: usize) -> Option<Unique<[Page]>> {
            (self.alloc_pages)(n)
      }

      /// # Safety
      ///
      /// It'll always be safe **ONLY IF** it's called single-thread, and its components
      /// won't fail.
      pub unsafe fn dealloc_pages(&mut self, pages: Unique<[Page]>) {
            (self.dealloc_pages)(pages)
      }
}
