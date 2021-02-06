use core::mem::MaybeUninit;
use core::ops::Range;
use core::ptr::NonNull;
use uefi::prelude::*;
use uefi::table::boot::{AllocateType, MemoryType};

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;

const IDENTITY_OFFSET: usize = 0;
static mut ROOT_TABLE: MaybeUninit<NonNull<[paging::Entry]>> = MaybeUninit::uninit();

struct BootAlloc<'a> {
      bs: &'a BootServices,
}

impl<'a> paging::alloc::PageAlloc for BootAlloc<'a> {
      fn alloc(&mut self) -> Option<paging::PAddr> {
            self.bs
                  .allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1)
                  .ok()
                  .and_then(|c| paging::PAddr::new(c.log() as usize))
      }

      fn dealloc(&mut self, addr: paging::PAddr) {
            let _ = self.bs.free_pages(addr.get() as u64, 1).log_warning();
      }
}

pub fn init() {
      let cr3: usize;
      unsafe { asm!("mov {}, cr3", out(reg) cr3) };

      let rt = NonNull::new(cr3 as *mut paging::Entry).expect("cr3 is zero");
      unsafe {
            ROOT_TABLE
                  .as_mut_ptr()
                  .write(NonNull::slice_from_raw_parts(rt, paging::NR_ENTRIES))
      };
}

pub fn maps(
      syst: &SystemTable<Boot>,
      virt: Range<paging::LAddr>,
      phys: paging::PAddr,
      attr: paging::Attr,
) -> Result<(), paging::Error> {
      let map_info = paging::MapInfo {
            virt,
            phys,
            attr,
            id_off: IDENTITY_OFFSET,
      };

      paging::maps(
            unsafe { ROOT_TABLE.assume_init() },
            &map_info,
            &mut BootAlloc {
                  bs: &syst.boot_services(),
            },
      )
}

pub fn unmaps(syst: &SystemTable<Boot>, virt: Range<paging::LAddr>) -> Result<(), paging::Error> {
      paging::unmaps(
            unsafe { ROOT_TABLE.assume_init() },
            virt,
            IDENTITY_OFFSET,
            &mut BootAlloc {
                  bs: &syst.boot_services(),
            },
      )
}

pub fn get_mmap(syst: &SystemTable<Boot>, buffer: &mut [u8]) {
      let (key, mmap) = syst
            .boot_services()
            .memory_map(buffer)
            .expect_success("Failed to get memory mappings");

      let mut addr_max = 0;

      for block in mmap {
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
