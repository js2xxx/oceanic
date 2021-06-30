#![no_std]
#![feature(const_btree_new)]

mod raw;

extern crate alloc;

const NR_INIT_TABLES: usize = 128;
static mut INIT_TABLES: [core::mem::MaybeUninit<raw::ACPI_TABLE_DESC>; NR_INIT_TABLES] =
      [core::mem::MaybeUninit::uninit(); NR_INIT_TABLES];

pub unsafe fn init_tables(rsdp: *const core::ffi::c_void) {
      raw::RSDP = rsdp;
      let _status =
            raw::AcpiInitializeTables(INIT_TABLES.as_mut_ptr().cast(), NR_INIT_TABLES as u32, 0);
}
