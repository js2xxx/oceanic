use core::{alloc::Layout, ptr::NonNull};

#[inline(never)]
unsafe fn alloc_pages(n: usize) -> Option<NonNull<[heap::Page]>> {
    let flags = {
        use sv_call::mem::Flags;
        Flags::READABLE | Flags::WRITABLE | Flags::EXECUTABLE | Flags::USER_ACCESS
    };
    let (layout, _) = Layout::new::<heap::Page>().repeat(n).ok()?;
    let phys = solvent::mem::Phys::allocate(layout, flags).ok()?;
    let ptr = solvent::mem::Space::current()
        .map(None, phys, 0, layout.size(), flags)
        .ok()?;
    Some(NonNull::slice_from_raw_parts(ptr.cast::<heap::Page>(), n))
}

#[inline(never)]
unsafe fn dealloc_pages(pages: NonNull<[heap::Page]>) {
    let ptr = pages.cast::<u8>();
    let _ = solvent::mem::Space::current().unmap(ptr);
}

pub fn init() {
    unsafe { heap::set_alloc(alloc_pages, dealloc_pages) };
    heap::test_global(unsafe { solvent::time::Instant::now().raw() as usize });
}
