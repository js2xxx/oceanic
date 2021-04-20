#![no_std]
#![feature(alloc_layout_extra)]
#![feature(allocator_api)]
#![feature(asm)]
#![feature(box_syntax)]
#![feature(const_fn_transmute)]
#![feature(default_alloc_error_handler)]
#![feature(lang_items)]
#![feature(maybe_uninit_ref)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]
#![feature(thread_local)]

mod cpu;
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

      // Tests
      l::debug!("Creating a space");
      let krl_space = mem::space::Space::new(mem::space::CreateType::Kernel);
      unsafe { krl_space.load() };

      l::debug!("Allocating GDT");
      let _gdt = cpu::seg::ndt::create_gdt(&krl_space);

      l::debug!("Allocating IDT");
      let _idt = cpu::seg::idt::create_idt(&krl_space);

      // Test end
      l::debug!("Reaching end of kernel");
}
