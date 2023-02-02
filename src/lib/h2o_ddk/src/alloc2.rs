use core::{
    alloc::{AllocError, Allocator, GlobalAlloc, Layout},
    ptr::NonNull,
};

use crate::ffi::vtable;

pub struct Ddk;

unsafe impl Allocator for Ddk {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let ptr = unsafe { (vtable().alloc)(layout.size(), layout.align()).cast() };
        NonNull::new(ptr)
            .map(|ptr| NonNull::slice_from_raw_parts(ptr, layout.size()))
            .ok_or(AllocError)
    }

    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        (vtable().dealloc)(ptr.as_ptr().cast(), layout.size(), layout.align())
    }
}

unsafe impl GlobalAlloc for Ddk {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        (vtable().alloc)(layout.size(), layout.align()).cast()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        (vtable().dealloc)(ptr.cast(), layout.size(), layout.align())
    }
}

#[global_allocator]
static DDK: Ddk = Ddk;
