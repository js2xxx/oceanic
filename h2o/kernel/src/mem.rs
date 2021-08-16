pub mod space;

use paging::LAddr;

use core::ptr::NonNull;

#[inline(never)]
unsafe fn alloc_pages(n: usize) -> Option<NonNull<[heap::Page]>> {
      let laddr = pmm::alloc_pages_exact(n, None)?.to_laddr(minfo::ID_OFFSET);
      let ptr = NonNull::new(laddr.cast::<heap::Page>());
      ptr.map(|ptr| NonNull::slice_from_raw_parts(ptr, n))
}

#[inline(never)]
unsafe fn dealloc_pages(pages: NonNull<[heap::Page]>) {
      let paddr = LAddr::new(pages.as_mut_ptr().cast()).to_paddr(minfo::ID_OFFSET);
      let n = pages.len();
      pmm::dealloc_pages_exact(n, paddr);
}

/// Initialize the PMM and the kernel heap (Rust global allocator).
pub fn init() {
      let all_available = pmm::init(
            crate::KARGS.efi_mmap_paddr,
            crate::KARGS.efi_mmap_len,
            crate::KARGS.efi_mmap_unit,
            minfo::TRAMPOLINE_RANGE,
      );
      log::info!(
            "Memory size: {:.3} GB ({:#x} Bytes)",
            (all_available as f64) / 1073741824.0,
            all_available
      );
      heap::set_alloc(alloc_pages, dealloc_pages);
      heap::test();
}
