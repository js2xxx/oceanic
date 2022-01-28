use core::{alloc::Layout, ptr::NonNull};

#[inline(never)]
unsafe fn alloc_pages(n: usize) -> Option<NonNull<[heap::Page]>> {
    let flags = {
        use solvent::mem::Flags;
        Flags::READABLE | Flags::WRITABLE | Flags::EXECUTABLE | Flags::USER_ACCESS
    };
    let (layout, _) = Layout::new::<heap::Page>().repeat(n).ok()?;
    let ptr = {
        let (size, align) = (layout.size(), layout.align());
        let phys = solvent::call::phys_alloc(size, align, flags.bits()).ok()?;
        let mi = solvent::mem::MapInfo {
            addr: 0,
            map_addr: false,
            phys,
            phys_offset: 0,
            len: size,
            flags: flags.bits(),
        };
        let ptr = solvent::call::mem_map(solvent::Handle::NULL, &mi).ok()?;
        let _ = solvent::call::obj_drop(phys);
        NonNull::slice_from_raw_parts(NonNull::new_unchecked(ptr), size)
    };
    Some(NonNull::slice_from_raw_parts(ptr.cast::<heap::Page>(), n))
}

#[inline(never)]
unsafe fn dealloc_pages(pages: NonNull<[heap::Page]>) {
    let ptr = pages.cast::<u8>();
    let _ = solvent::call::mem_unmap(solvent::Handle::NULL, ptr.as_ptr());
}

pub fn init() {
    unsafe { heap::set_alloc(alloc_pages, dealloc_pages) };
    let mut time = 0;
    solvent::call::get_time(&mut time).expect("Failed to get time");
    heap::test_global(time as usize);
}
