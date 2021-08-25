#![no_std]

use core::alloc::Layout;

#[derive(Debug, Copy, Clone)]
pub struct KernelArgs {
      pub rsdp: paging::PAddr,

      pub efi_mmap_paddr: paging::PAddr,
      pub efi_mmap_len: usize,
      pub efi_mmap_unit: usize,

      pub pls_layout: Option<Layout>,

      pub tinit_phys: paging::PAddr,
      pub tinit_len: usize,
}