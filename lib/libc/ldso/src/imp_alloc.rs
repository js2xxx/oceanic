use alloc::alloc::handle_alloc_error;
use core::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::{AtomicUsize, Ordering::*},
};

use solvent::prelude::{Flags, Phys, Space, PAGE_SIZE};

const DL_ALLOC_BASE: usize = 0x7F2C_0000_0000;

#[global_allocator]
static DL_ALLOC: DlAlloc = DlAlloc {
    top: AtomicUsize::new(DL_ALLOC_BASE),
    end: AtomicUsize::new(DL_ALLOC_BASE),
};

pub struct DlAlloc {
    top: AtomicUsize,
    end: AtomicUsize,
}

unsafe impl GlobalAlloc for DlAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut cur = self.top.load(Acquire);
        let (next, next_end) = loop {
            let next = cur.next_multiple_of(layout.align());
            let new = next + layout.size();
            match self.top.compare_exchange(cur, new, AcqRel, Acquire) {
                Ok(_) => break (next, new),
                Err(c) => cur = c,
            }
        };

        let flags = Flags::READABLE | Flags::WRITABLE | Flags::EXECUTABLE | Flags::USER_ACCESS;
        let mut end = self.end.load(Acquire);
        loop {
            if next_end <= end {
                break next as *mut u8;
            }

            let size = (next_end - end).next_multiple_of(PAGE_SIZE);
            let layout = unsafe { Layout::from_size_align_unchecked(size, PAGE_SIZE) };
            let res = Phys::allocate(layout, flags).and_then(|phys| {
                Space::current().map_ref(Some(end), phys.into_ref(layout.size()), flags)
            });
            let next_end = match res {
                Ok(mut ptr) => unsafe { ptr.as_mut().as_mut_ptr_range().end as usize },
                Err(_) => handle_alloc_error(layout),
            };

            match self.end.compare_exchange(end, next_end, AcqRel, Acquire) {
                Ok(_) => break next as *mut u8,
                Err(cur) => end = cur,
            }
        }
    }

    // Leaks memory
    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
}
