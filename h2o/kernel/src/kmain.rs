#![no_std]
#![feature(asm)]
#![feature(box_syntax)]
#![feature(default_alloc_error_handler)]
#![feature(lang_items)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]

mod log;
mod mem;
mod rxx;

use ::log as l;

extern crate alloc;

#[no_mangle]
pub extern "C" fn kmain(
      rsdp: *const core::ffi::c_void,
      efi_mmap_paddr: paging::PAddr,
      efi_mmap_len: usize,
      efi_mmap_unit: usize,
      tls_size: usize,
) {
      self::log::init(l::Level::Debug);
      l::info!("kmain: Starting initialization");

      mem::init(efi_mmap_paddr, efi_mmap_len, efi_mmap_unit);

      l::debug!("Reaching end of kernel");
}
