use core::alloc::Layout;
use core::mem::size_of;
use core::ptr::{null_mut, NonNull};

#[inline(never)]
unsafe fn alloc_pages(n: usize) -> Option<NonNull<[heap::Page]>> {
      let flags = {
            use solvent::mem::Flags;
            Flags::READABLE | Flags::WRITABLE | Flags::EXECUTABLE | Flags::USER_ACCESS
      };
      let (layout, _) = Layout::new::<heap::Page>().repeat(n).ok()?;
      let ptr = solvent::mem::alloc_pages(null_mut(), 0, layout, flags).ok()?;
      let ptr = NonNull::new(ptr.cast::<heap::Page>());
      ptr.map(|ptr| NonNull::slice_from_raw_parts(ptr, n))
}

#[inline(never)]
unsafe fn dealloc_pages(pages: NonNull<[heap::Page]>) {
      let ptr = {
            let ptr = pages.as_ptr().cast::<u8>();
            let n = pages.len();
            core::slice::from_raw_parts_mut(ptr, n * size_of::<heap::Page>())
      };
      let _ = solvent::mem::dealloc_pages(ptr, true);
}

pub fn init() {
      unsafe { heap::set_alloc(alloc_pages, dealloc_pages) };
      heap::test(unsafe { solvent::time::Instant::now().raw() } as usize);
}
