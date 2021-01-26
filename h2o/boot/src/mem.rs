use alloc::vec;
use uefi::prelude::*;

const PAGE_SHIFT: usize = 12;
const PAGE_SIZE: usize = 1 << PAGE_SHIFT;

pub fn get_mapping(syst: &SystemTable<Boot>) {
      let mut buffer = vec![0u8; 4096];

      let (_key, mmap) = syst
            .boot_services()
            .memory_map(&mut buffer)
            .expect_success("Failed to get memory mappings");

      let mut addr_max = 0;

      for block in mmap {
            // log::info!(
            //       "{:?}:\t ({:x})\t{:x} `\t {:x}",
            //       block.ty,
            //       block.virt_start,
            //       block.phys_start,
            //       block.page_count * 4096
            // );

            addr_max = core::cmp::max(
                  addr_max,
                  block.phys_start + (block.page_count << PAGE_SHIFT),
            );
      }

      assert!(addr_max > 0);
}

pub fn get_acpi_rsdp(syst: &SystemTable<Boot>) -> *const core::ffi::c_void {
      use uefi::table::cfg::*;
      let cfgs = syst.config_table();
      for cfg in cfgs {
            if matches!(cfg.guid, ACPI2_GUID | ACPI_GUID) {
                  return cfg.address;
            }
      }
      panic!("Failed to get RSDP")
}
