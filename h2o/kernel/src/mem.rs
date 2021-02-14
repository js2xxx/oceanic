pub mod space;

pub fn init(efi_mmap_paddr: paging::PAddr, efi_mmap_len: usize, efi_mmap_unit: usize) {
      pmm::init(efi_mmap_paddr, efi_mmap_len, efi_mmap_unit);
}
