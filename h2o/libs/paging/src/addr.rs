use core::{
    alloc::Layout,
    num::NonZeroUsize,
    ops::{Deref, DerefMut, Range},
    ptr::NonNull,
};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
#[repr(transparent)]
pub struct PAddr(usize);

impl PAddr {
    #[inline]
    pub const fn new(addr: usize) -> Self {
        PAddr(addr)
    }

    pub const fn as_non_zero(self) -> Option<NonZeroUsize> {
        NonZeroUsize::new(self.0)
    }

    #[inline]
    pub fn to_laddr(self, id_off: usize) -> LAddr {
        LAddr::from(self.0 + id_off)
    }

    pub fn in_page_offset(self) -> usize {
        self.0 & crate::PAGE_MASK
    }
}

impl Deref for PAddr {
    type Target = usize;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PAddr {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl core::fmt::Debug for PAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PAddr({:#x})", self.0)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct LAddr(*mut u8);

impl LAddr {
    #[inline]
    pub const fn new(ptr: *mut u8) -> Self {
        LAddr(ptr)
    }

    #[inline]
    pub fn val(self) -> usize {
        self.0 as usize
    }

    #[inline]
    pub fn as_non_null(self) -> Option<NonNull<u8>> {
        NonNull::new(self.0)
    }

    /// # Safety
    ///
    /// `self` must be non-null.
    #[inline]
    pub unsafe fn as_non_null_unchecked(self) -> NonNull<u8> {
        NonNull::new_unchecked(self.0)
    }

    #[inline]
    pub fn to_paddr(self, id_off: usize) -> PAddr {
        PAddr(self.val() - id_off)
    }

    pub fn in_page_offset(self) -> usize {
        self.val() & crate::PAGE_MASK
    }

    pub(crate) fn advance(&mut self, offset: usize) {
        self.0 = unsafe { self.0.add(offset) };
    }

    pub fn to_range(self, layout: Layout) -> Range<Self> {
        self..Self(self.wrapping_add(layout.size()))
    }
}

impl Deref for LAddr {
    type Target = *mut u8;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for LAddr {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<usize> for LAddr {
    #[inline]
    fn from(val: usize) -> Self {
        LAddr(val as *mut u8)
    }
}

impl From<u64> for LAddr {
    #[inline]
    fn from(val: u64) -> Self {
        LAddr(val as *mut u8)
    }
}

impl<T> From<*const T> for LAddr {
    #[inline]
    fn from(val: *const T) -> Self {
        LAddr(val as _)
    }
}

impl<T> From<*mut T> for LAddr {
    #[inline]
    fn from(val: *mut T) -> Self {
        LAddr(val as _)
    }
}

impl<T: ?Sized> From<NonNull<T>> for LAddr {
    #[inline]
    fn from(ptr: NonNull<T>) -> Self {
        LAddr(ptr.as_ptr().cast())
    }
}
