use core::mem::MaybeUninit;
use core::ops::Range;
use core::ptr::NonNull;
use paging::PageAlloc;
use uefi::prelude::*;
use uefi::table::boot::{AllocateType, MemoryType};

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;

const EFI_ID_OFFSET: usize = 0;
const KERNEL_ID_OFFSET: usize = 0xFFFF_8000_0000_0000;
static mut ROOT_TABLE: MaybeUninit<NonNull<[paging::Entry]>> = MaybeUninit::uninit();

pub struct BootAlloc<'a> {
      bs: &'a BootServices,
}

impl<'a> BootAlloc<'a> {
      pub fn alloc_n(&mut self, n: usize) -> Option<paging::PAddr> {
            self.bs
                  .allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, n)
                  .ok()
                  .map(|c| paging::PAddr::new(c.log() as usize))
      }
}

impl<'a> paging::alloc::PageAlloc for BootAlloc<'a> {
      fn alloc(&mut self) -> Option<paging::PAddr> {
            self.alloc_n(1)
      }

      fn dealloc(&mut self, addr: paging::PAddr) {
            let _ = self.bs.free_pages(*addr as u64, 1).log_warning();
      }
}

pub fn init(syst: &SystemTable<Boot>) {
      log::trace!("mem::init: syst = {:?}", syst as *const _);

      let rt_addr = alloc(syst)
            .alloc_zeroed(EFI_ID_OFFSET)
            .expect("Failed to allocate a page");
      let rt = unsafe { NonNull::new_unchecked(*rt_addr as *mut paging::Entry) };

      unsafe {
            ROOT_TABLE
                  .as_mut_ptr()
                  .write(NonNull::slice_from_raw_parts(rt, paging::NR_ENTRIES))
      };

      let phys = paging::PAddr::new(0);
      let virt = paging::LAddr::from(0)..paging::LAddr::from(0x1_0000_0000);
      let pg_attr = paging::Attr::KERNEL_RW;

      log::trace!(
            "mapping kernel's pages 0 ~ 4G: root_phys = {:?}",
            rt.as_ptr()
      );
      maps(syst, virt, phys, pg_attr).expect("Failed to map virtual memory");
}

pub fn alloc(syst: &SystemTable<Boot>) -> BootAlloc {
      log::trace!("mem::alloc: syst = {:?}", syst as *const _);
      BootAlloc {
            bs: &syst.boot_services(),
      }
}

pub fn maps(
      syst: &SystemTable<Boot>,
      virt: Range<paging::LAddr>,
      phys: paging::PAddr,
      attr: paging::Attr,
) -> Result<(), paging::Error> {
      log::trace!(
            "mem::maps: syst = {:?}, virt = {:?}, phys = {:?}, attr = {:?}",
            syst as *const _,
            virt,
            phys,
            attr
      );

      let map_info = paging::MapInfo {
            virt,
            phys,
            attr,
            id_off: EFI_ID_OFFSET,
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
      log::trace!(
            "mem::unmaps: syst = {:?}, virt = {:?}",
            syst as *const _,
            virt,
      );
      
      paging::unmaps(
            unsafe { ROOT_TABLE.assume_init() },
            virt,
            EFI_ID_OFFSET,
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
