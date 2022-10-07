mod phys;
mod space;
mod virt;

use core::{
    alloc::Layout,
    marker::PhantomData,
    mem::{self, MaybeUninit},
    ops::{Deref, DerefMut},
    slice,
};

pub use sv_call::mem::Flags;
use sv_call::mem::IoVec;

pub use self::{phys::*, space::Space, virt::Virt};

cfg_if::cfg_if! { if #[cfg(target_arch = "x86_64")] {

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
pub const PAGE_MASK: usize = PAGE_SIZE - 1;

}}
// SAFETY: Both the size and the alignment are 2^n-bounded.
pub const PAGE_LAYOUT: Layout = unsafe { Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE) };

#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
pub struct IoSlice<'a> {
    raw: IoVec,
    _marker: PhantomData<&'a [u8]>,
}

unsafe impl Send for IoSlice<'_> {}
unsafe impl Sync for IoSlice<'_> {}

impl<'a> IoSlice<'a> {
    #[inline]
    pub fn new(data: &'a [u8]) -> Self {
        IoSlice {
            raw: IoVec {
                ptr: data.as_ptr() as *mut u8,
                len: data.len(),
            },
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn advance(&mut self, n: usize) {
        if self.raw.len < n {
            panic!("advancing IoSlice beyond its length");
        }

        unsafe {
            self.raw.len -= n;
            self.raw.ptr = self.raw.ptr.add(n);
        }
    }

    #[inline]
    pub fn advance_slices(bufs: &mut &mut [IoSlice<'a>], n: usize) {
        // Number of buffers to remove.
        let mut remove = 0;
        // Total length of all the to be removed buffers.
        let mut accumulated_len = 0;
        for buf in bufs.iter() {
            if accumulated_len + buf.len() > n {
                break;
            } else {
                accumulated_len += buf.len();
                remove += 1;
            }
        }

        *bufs = &mut mem::take(bufs)[remove..];
        if bufs.is_empty() {
            assert!(
                n == accumulated_len,
                "advancing io slices beyond their length"
            );
        } else {
            bufs[0].advance(n - accumulated_len)
        }
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.raw.ptr, self.raw.len) }
    }
}

impl<'a> Deref for IoSlice<'a> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct IoSliceMut<'a> {
    raw: IoVec,
    _marker: PhantomData<&'a mut [u8]>,
}

unsafe impl<'a> Send for IoSliceMut<'a> {}
unsafe impl<'a> Sync for IoSliceMut<'a> {}

impl<'a> IoSliceMut<'a> {
    #[inline]
    pub fn new(data: &'a mut [u8]) -> Self {
        IoSliceMut {
            raw: IoVec {
                ptr: data.as_mut_ptr(),
                len: data.len(),
            },
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn uninit(data: &'a mut [MaybeUninit<u8>]) -> Self {
        IoSliceMut {
            raw: IoVec {
                ptr: data.as_mut_ptr() as _,
                len: data.len(),
            },
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn advance(&mut self, n: usize) {
        if self.raw.len < n {
            panic!("advancing IoSlice beyond its length");
        }

        unsafe {
            self.raw.len -= n;
            self.raw.ptr = self.raw.ptr.add(n);
        }
    }

    #[inline]
    pub fn advance_slices(bufs: &mut &mut [IoSlice<'a>], n: usize) {
        // Number of buffers to remove.
        let mut remove = 0;
        // Total length of all the to be removed buffers.
        let mut accumulated_len = 0;
        for buf in bufs.iter() {
            if accumulated_len + buf.len() > n {
                break;
            } else {
                accumulated_len += buf.len();
                remove += 1;
            }
        }

        *bufs = &mut mem::take(bufs)[remove..];
        if bufs.is_empty() {
            assert!(
                n == accumulated_len,
                "advancing io slices beyond their length"
            );
        } else {
            bufs[0].advance(n - accumulated_len)
        }
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.raw.ptr, self.raw.len) }
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.raw.ptr, self.raw.len) }
    }
}

impl<'a> Deref for IoSliceMut<'a> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<'a> DerefMut for IoSliceMut<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}
