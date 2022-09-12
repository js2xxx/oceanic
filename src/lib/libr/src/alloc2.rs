use core::{alloc::Layout, mem, ptr::NonNull};

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

pub(crate) unsafe fn init() {
    unsafe { heap::set_alloc(alloc_pages, dealloc_pages) };
    #[cfg(debug_assertions)]
    heap::test_global(unsafe { solvent::time::Instant::now().raw() as usize });
}

/// The function indicating memory runs out.
#[alloc_error_handler]
fn rust_oom(layout: core::alloc::Layout) -> ! {
    log::error!("Allocation error for {:?}", layout);

    loop {
        unsafe { core::arch::asm!("pause; ud2") }
    }
}
