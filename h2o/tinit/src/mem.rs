use core::{alloc::Layout, ptr::NonNull};

#[inline(never)]
unsafe fn alloc_pages(n: usize) -> Option<NonNull<[heap::Page]>> {
    let flags = {
        use solvent::mem::Flags;
        Flags::READABLE | Flags::WRITABLE | Flags::EXECUTABLE | Flags::USER_ACCESS
    };
    let (layout, _) = Layout::new::<heap::Page>().repeat(n).ok()?;
    let ptr = solvent::mem::mem_alloc(layout, flags).ok()?;
    Some(NonNull::slice_from_raw_parts(ptr.cast::<heap::Page>(), n))
}

#[inline(never)]
unsafe fn dealloc_pages(pages: NonNull<[heap::Page]>) {
    let ptr = pages.cast::<u8>();
    let _ = solvent::mem::mem_dealloc(ptr);
}

pub fn init() {
    unsafe { heap::set_alloc(alloc_pages, dealloc_pages) };
    heap::test(unsafe { solvent::time::Instant::now().raw() } as usize);
}
