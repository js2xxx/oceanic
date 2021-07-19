#![no_std]
#![allow(unused_unsafe)]
#![feature(alloc_layout_extra)]
#![feature(allocator_api)]
#![feature(asm)]
#![feature(box_syntax)]
#![feature(concat_idents)]
#![feature(const_fn_fn_ptr_basics)]
#![feature(const_fn_transmute)]
#![feature(default_alloc_error_handler)]
#![feature(lang_items)]
#![feature(macro_attributes_in_derive_output)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]
#![feature(thread_local)]
#![feature(trace_macros)]
#![feature(vec_into_raw_parts)]

pub mod cpu;
pub mod dev;
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
      _tls_size: usize,
) {
      unsafe { cpu::set_id() };

      // SAFE: Everything is uninitialized.
      unsafe { self::log::init(l::Level::Debug) };
      l::info!("Starting initialization");

      mem::init(efi_mmap_paddr, efi_mmap_len, efi_mmap_unit);

      l::debug!("Creating the kernel space");
      unsafe { mem::space::init_kernel() };

      l::debug!("Initializing ACPI tables");
      unsafe { acpi::init_tables(rsdp) };
      let lapic_data = unsafe { acpi::table::get_lapic_data() }.expect("Failed to get LAPIC data");
      let ioapic_data =
            unsafe { acpi::table::get_ioapic_data() }.expect("Failed to get IOAPIC data");

      l::debug!("Set up CPU architecture");
      unsafe { cpu::arch::init(lapic_data, ioapic_data) };

      // Tests

      // Test end
      l::debug!("Reaching end of kernel");
}
