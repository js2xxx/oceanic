use core::alloc::{GlobalAlloc, Layout};

#[global_allocator]
static NULL_ALLOC: NullAlloc = NullAlloc;

pub struct NullAlloc;

unsafe impl GlobalAlloc for NullAlloc {
    unsafe fn alloc(&self, _: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
}
