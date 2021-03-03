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

use paging::{LAddr, PAddr};

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
      let _u = box 1;

      let flags = mem::extent::Flags::READABLE | mem::extent::Flags::WRTIEABLE;

      l::debug!("Creating a space");
      let krl_space =
            mem::space::Space::new(mem::space::CreateType::Kernel, mem::extent::Flags::all());

      l::debug!("Creating a region");
      let region = krl_space
            .extent()
            .create_subregion(
                  minfo::KERNEL_ALLOCABLE_RANGE.start
                        ..LAddr::from(minfo::KERNEL_ALLOCABLE_RANGE.start.val() + 0x100000),
                  mem::extent::Flags::READABLE | mem::extent::Flags::WRTIEABLE,
            )
            .expect("Failed to create region");

      l::debug!("Creating an object");
      let mut obj = mem::pobj::PObject::new(flags);
      obj.add_range(PAddr::new(0)..PAddr::new(0x1000));

      l::debug!("Creating a mapping");
      let mapping = region
            .create_mapping(obj, true)
            .expect("Failed to create mapping");

      l::debug!("Unmapping");
      mapping
            .decommit_mapping()
            .expect("Failed to decommit a mapping");

      // Test end
      
      l::debug!("Reaching end of kernel");
}
