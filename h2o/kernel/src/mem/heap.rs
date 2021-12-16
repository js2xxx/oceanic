use alloc::alloc::handle_alloc_error;
use core::{
    alloc::{Allocator, GlobalAlloc, Layout},
    ptr::NonNull,
};

use ::heap::Allocator as Memory;
use paging::LAddr;

use crate::sched::PREEMPT;

#[global_allocator]
static KH: KHeap = KHeap {
    global_mem: Memory::new(alloc_pages, dealloc_pages),
};

struct KHeap {
    global_mem: Memory,
}

unsafe impl Send for KHeap {}
unsafe impl Sync for KHeap {}

unsafe impl GlobalAlloc for KHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _pree = PREEMPT.lock();
        self.global_mem
            .allocate(layout)
            .map_or_else(|_| handle_alloc_error(layout), |ptr| ptr.as_mut_ptr())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let ptr = match NonNull::new(ptr) {
            Some(ptr) => ptr,
            None => return,
        };
        let _pree = PREEMPT.lock();
        self.global_mem.deallocate(ptr, layout)
    }
}

#[inline(never)]
unsafe fn alloc_pages(n: usize) -> Option<NonNull<[::heap::Page]>> {
    let laddr = pmm::alloc_pages_exact(n, None)?.to_laddr(minfo::ID_OFFSET);
    let ptr = NonNull::new(laddr.cast::<::heap::Page>());
    ptr.map(|ptr| NonNull::slice_from_raw_parts(ptr, n))
}

#[inline(never)]
unsafe fn dealloc_pages(pages: NonNull<[::heap::Page]>) {
    let paddr = LAddr::new(pages.as_ptr().cast()).to_paddr(minfo::ID_OFFSET);
    let n = pages.len();
    pmm::dealloc_pages_exact(n, paddr);
}

pub(super) fn init_global() {
    ::heap::test(&KH, archop::rand::get() as usize);
}
