use core::{alloc::Layout, ptr::NonNull};

#[inline(never)]
unsafe fn alloc_pages(n: usize) -> Option<NonNull<[heap::Page]>> {
    let flags = {
        use sv_call::mem::Flags;
        Flags::READABLE | Flags::WRITABLE | Flags::EXECUTABLE | Flags::USER_ACCESS
    };
    let (layout, _) = Layout::new::<heap::Page>().repeat(n).ok()?;
    let ptr = {
        let (size, align) = (layout.size(), layout.align());
        let phys = sv_call::call::sv_phys_alloc(size, align, flags)
            .into_res()
            .ok()?;
        let mi = sv_call::mem::MapInfo {
            addr: 0,
            map_addr: false,
            phys,
            phys_offset: 0,
            len: size,
            flags,
        };
        let ptr = sv_call::call::sv_mem_map(sv_call::Handle::NULL, &mi)
            .into_res()
            .ok()? as *mut u8;
        let _ = sv_call::call::sv_obj_drop(phys);
        NonNull::slice_from_raw_parts(NonNull::new_unchecked(ptr), size)
    };
    Some(NonNull::slice_from_raw_parts(ptr.cast::<heap::Page>(), n))
}

#[inline(never)]
unsafe fn dealloc_pages(pages: NonNull<[heap::Page]>) {
    let ptr = pages.cast::<u8>();
    let _ = sv_call::call::sv_mem_unmap(sv_call::Handle::NULL, ptr.as_ptr());
}

pub fn init() {
    unsafe { heap::set_alloc(alloc_pages, dealloc_pages) };
    let mut time = 0u128;
    sv_call::call::sv_time_get(&mut time as *mut _ as *mut _)
        .into_res()
        .expect("Failed to get time");
    heap::test_global(time as usize);
}
