use core::alloc::Layout;
use core::mem::size_of;
use core::ptr::{null_mut, Unique};

#[inline(never)]
unsafe fn alloc_pages(n: usize) -> Option<Unique<[heap::Page]>> {
      let flags = {
            use solvent::mem::Flags;
            Flags::READABLE | Flags::WRITABLE | Flags::EXECUTABLE | Flags::USER_ACCESS
      };
      let (layout, _) = Layout::new::<heap::Page>().repeat(n).ok()?;
      let ptr = solvent::mem::alloc_pages(null_mut(), 0, layout, flags).ok()?;
      let ptr = Unique::new(ptr.cast::<heap::Page>());
      ptr.map(|ptr| Unique::from(core::slice::from_raw_parts_mut(ptr.as_ptr(), n)))
}

#[inline(never)]
unsafe fn dealloc_pages(pages: Unique<[heap::Page]>) {
      let ptr = {
            let ptr = pages.as_ptr().cast::<u8>();
            let n = pages.as_ref().len();
            core::slice::from_raw_parts_mut(ptr, n * size_of::<heap::Page>())
      };
      let _ = solvent::mem::dealloc_pages(ptr, true);
}

pub fn init() {
      heap::set_alloc(alloc_pages, dealloc_pages);
      heap::test();
}
