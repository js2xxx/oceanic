use core::num::NonZeroUsize;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PAddr(usize);

impl PAddr {
      pub const fn new(addr: usize) -> Self {
            PAddr(addr)
      }

      pub const fn as_non_zero(self) -> Option<NonZeroUsize> {
            NonZeroUsize::new(self.0)
      }

      pub fn to_laddr(self, id_off: usize) -> LAddr {
            LAddr::from(self.0 + id_off)
      }

      pub fn in_page_offset(self) -> usize {
            self.0 & crate::PAGE_MASK
      }
}

impl Deref for PAddr {
      type Target = usize;

      fn deref(&self) -> &Self::Target {
            &self.0
      }
}

impl DerefMut for PAddr {
      fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
      }
}

impl core::fmt::Debug for PAddr {
      fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "PAddr({:#x})", self.0)
      }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct LAddr(*mut u8);

impl LAddr {
      pub const fn new(ptr: *mut u8) -> Self {
            LAddr(ptr)
      }

      pub fn val(self) -> usize {
            self.0 as usize
      }

      pub fn as_non_null(self) -> Option<NonNull<u8>> {
            NonNull::new(self.0)
      }

      pub fn to_paddr(self, id_off: usize) -> PAddr {
            PAddr(self.val() - id_off)
      }

      pub fn in_page_offset(self) -> usize {
            self.val() & crate::PAGE_MASK
      }

      pub(crate) fn advance(&mut self, offset: usize) {
            self.0 = unsafe { self.0.add(offset) };
      }
}

impl Deref for LAddr {
      type Target = *mut u8;

      fn deref(&self) -> &Self::Target {
            &self.0
      }
}

impl DerefMut for LAddr {
      fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
      }
}

impl From<usize> for LAddr {
      fn from(val: usize) -> Self {
            LAddr(val as *mut u8)
      }
}
