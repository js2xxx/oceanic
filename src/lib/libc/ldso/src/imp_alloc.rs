use alloc::alloc::handle_alloc_error;
use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    sync::atomic::{AtomicUsize, Ordering::*},
};

use solvent::prelude::*;

const DL_ALLOC_BASE: usize = 0x7F2C_0000_0000;

#[global_allocator]
static DL_ALLOC: DlAlloc2 = DlAlloc2 {
    buffer: UnsafeCell::new(Buffer([0; BUFFER_SIZE])),
    buffer_index: UnsafeCell::new(0),
    inner: DlAlloc {
        top: AtomicUsize::new(DL_ALLOC_BASE),
        end: AtomicUsize::new(DL_ALLOC_BASE),
    },
};

struct DlAlloc {
    top: AtomicUsize,
    end: AtomicUsize,
}

impl DlAlloc {
    fn alloc(&self, layout: Layout, root_virt: &Virt) -> *mut u8 {
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
            let res = Phys::allocate(size, false).and_then(|phys| {
                let base = root_virt.base().as_ptr() as usize;
                root_virt.map_phys(Some(end - base), phys, flags)
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

    fn dealloc(&self, _: *mut u8, _: Layout, _: &Virt) {}
}

const BUFFER_SIZE: usize = 512;
#[repr(align(4096))]
struct Buffer([u8; BUFFER_SIZE]);
pub struct DlAlloc2 {
    buffer: UnsafeCell<Buffer>,
    buffer_index: UnsafeCell<usize>,
    inner: DlAlloc,
}

unsafe impl Send for DlAlloc2 {}
unsafe impl Sync for DlAlloc2 {}

unsafe impl GlobalAlloc for DlAlloc2 {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match svrt::try_get_root_virt() {
            Ok(root_virt) => self.inner.alloc(layout, root_virt),
            Err(_) => {
                let index = self.buffer_index.get();
                let i = (*index).next_multiple_of(layout.align());
                if i + layout.size() >= BUFFER_SIZE {
                    handle_alloc_error(layout);
                } else {
                    let ptr = self.buffer.get().cast::<u8>().add(i);
                    *index = i + layout.size();
                    ptr
                }
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if let Ok(root_virt) = svrt::try_get_root_virt() {
            let buffer = &*self.buffer.get();
            if !buffer.0.as_ptr_range().contains(&(ptr as *const _)) {
                self.inner.dealloc(ptr, layout, root_virt);
            }
        }
    }
}
