pub mod extent;
pub mod space;
pub mod pobj;

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

pub fn init(efi_mmap_paddr: paging::PAddr, efi_mmap_len: usize, efi_mmap_unit: usize) {
      pmm::init(efi_mmap_paddr, efi_mmap_len, efi_mmap_unit);
      heap::set_alloc(alloc_pages, dealloc_pages);
}
