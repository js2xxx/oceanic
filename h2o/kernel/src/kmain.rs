#![no_std]
#![allow(unused_unsafe)]
#![feature(alloc_layout_extra)]
#![feature(allocator_api)]
#![feature(asm)]
#![feature(box_syntax)]
#![feature(concat_idents)]
#![feature(const_fn_fn_ptr_basics)]
#![feature(const_fn_transmute)]
#![feature(const_raw_ptr_to_usize_cast)]
#![feature(default_alloc_error_handler)]
#![feature(lang_items)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]
#![feature(thread_local)]
#![feature(trace_macros)]
#![feature(vec_into_raw_parts)]

pub mod cpu;
pub mod log;
pub mod mem;
pub mod rxx;

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
      l::info!("Starting initialization");

      mem::init(efi_mmap_paddr, efi_mmap_len, efi_mmap_unit);

      // Tests
      l::debug!("Creating a space");
      let krl_space = mem::space::Space::new(mem::space::CreateType::Kernel);
      unsafe { krl_space.load() };

      l::debug!("Creating the CPU core");
      let arch_data = unsafe { cpu::arch::init(&krl_space) };

      unsafe { acpi::init_tables(rsdp) };

      let data = unsafe { acpi::table::get_lapic_data() };
      l::debug!("{:x?}", data);
      // unsafe {
      //       asm!("mov rax, 0; mov rdx, 0; mov rcx, 0; div rcx");
      // }

      // Test end
      l::debug!("Reaching end of kernel");
}
