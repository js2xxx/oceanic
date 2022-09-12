use alloc::alloc::handle_alloc_error;
use core::{
    alloc::{Allocator, GlobalAlloc, Layout},
    cell::UnsafeCell,
    mem,
    ptr::{self, NonNull},
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
};

#[inline(never)]
unsafe fn alloc_pages(n: usize) -> Option<NonNull<[heap::Page]>> {
    let flags = {
        use solvent::mem::Flags;
        Flags::READABLE | Flags::WRITABLE | Flags::EXECUTABLE | Flags::USER_ACCESS
    };
    let (layout, _) = Layout::new::<heap::Page>().repeat(n).ok()?;
    let phys = solvent::mem::Phys::allocate(layout.size(), false).ok()?;
    let ptr = svrt::root_virt().map(None, phys, 0, layout, flags).ok()?;
    Some(NonNull::slice_from_raw_parts(ptr.cast::<heap::Page>(), n))
}

#[inline(never)]
unsafe fn dealloc_pages(pages: NonNull<[heap::Page]>) {
    let ptr = pages.cast::<u8>();
    let size = pages.len() * mem::size_of::<heap::Page>();
    let _ = svrt::root_virt().unmap(ptr, size, false);
}

#[global_allocator]
static ALLOC: Alloc = Alloc {
    memory: heap::Allocator::new(alloc_pages, dealloc_pages),
    temp_buffer: UnsafeCell::new([0; BUFFER_SIZE]),
    temp_index: AtomicUsize::new(0),
};

#[thread_local]
static mut TCACHE: heap::ThreadCache = heap::ThreadCache::new();

const BUFFER_SIZE: usize = 4096;
struct Alloc {
    memory: heap::Allocator,
    temp_buffer: UnsafeCell<[u8; BUFFER_SIZE]>,
    temp_index: AtomicUsize,
}

// We ensures that `temp_buffer` won't be used after SVRT is initialized.
unsafe impl Sync for Alloc {}

unsafe impl GlobalAlloc for Alloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match svrt::try_get_root_virt() {
            Ok(_) => match TCACHE.allocate(layout, self.memory.pool()) {
                Ok(addr) => *addr,
                Err(_) => self
                    .memory
                    .allocate(layout)
                    .map_or(ptr::null_mut(), |ptr| ptr.as_ptr() as _),
            },
            Err(_) => {
                let index = self.temp_index.load(Relaxed);
                let i = index.next_multiple_of(layout.align());
                if i + layout.size() >= BUFFER_SIZE {
                    handle_alloc_error(layout)
                } else {
                    let ptr = self.temp_buffer.get().cast::<u8>().add(i);
                    self.temp_index.store(i + layout.size(), Relaxed);
                    ptr
                }
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let range = (*self.temp_buffer.get()).as_ptr_range();
        if !range.contains(&(ptr as _)) {
            if let Ok(Some(page)) = TCACHE.deallocate(ptr.into(), layout, self.memory.pool()) {
                let mut pager = self.memory.pager().lock();
                pager.dealloc_pages(NonNull::slice_from_raw_parts(page, 1))
            }
        }
    }
}

/// The function indicating memory runs out.
#[alloc_error_handler]
fn rust_oom(layout: core::alloc::Layout) -> ! {
    log::error!("Allocation error for {:?}", layout);

    loop {
        unsafe { core::arch::asm!("pause; ud2") }
    }
}
