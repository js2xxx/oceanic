use bitop_ex::BitOpEx;
use minfo::{ID_OFFSET as KERNEL_ID_OFFSET, INITIAL_ID_SPACE, KMEM_PHYS_BASE, PF_SIZE};
use paging::PageAlloc;

use core::mem::MaybeUninit;
use core::ops::Range;
use core::ptr::NonNull;
use uefi::prelude::*;
use uefi::table::boot;

pub const EFI_ID_OFFSET: usize = 0;
static mut ROOT_TABLE: MaybeUninit<NonNull<[paging::Entry]>> = MaybeUninit::uninit();

// pub enum MemoryType {
//       Free,
//       Acpi,
//       Mmio,
// }

// pub struct MemoryBlock {
//       ty: MemoryType,
//       range: Range<paging::PAddr>,
// }

pub struct BootAlloc<'a> {
      bs: &'a BootServices,
}

impl<'a> BootAlloc<'a> {
      pub fn alloc_n(&mut self, n: usize) -> Option<paging::PAddr> {
            self.bs
                  .allocate_pages(
                        boot::AllocateType::AnyPages,
                        boot::MemoryType::LOADER_DATA,
                        n,
                  )
                  .ok()
                  .map(|c| paging::PAddr::new(c.log() as usize))
      }

      pub fn alloc_into_slice(
            &mut self,
            size: usize,
            id_off: usize,
      ) -> Option<(paging::PAddr, *mut [u8])> {
            let n = size.div_ceil_bit(paging::PAGE_SHIFT);
            log::trace!(
                  "mem::BootAlloc::alloc_into_slice: size = {:?}, n = {:?}",
                  size,
                  n
            );

            let paddr = self.alloc_n(n)?;
            Some((paddr, unsafe {
                  core::slice::from_raw_parts_mut(*paddr.to_laddr(id_off), size)
            }))
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
      let virt_efi = paging::LAddr::from(EFI_ID_OFFSET)
            ..paging::LAddr::from(INITIAL_ID_SPACE + EFI_ID_OFFSET);
      let pg_attr = paging::Attr::KERNEL_RW;

      log::trace!(
            "mapping kernel's pages 0 ~ 4G: root_phys = {:?}",
            rt.as_ptr()
      );
      maps(syst, virt_efi, phys, pg_attr).expect("Failed to map virtual memory for H2O boot");
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

#[allow(dead_code)]
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

pub fn init_pf(syst: &SystemTable<Boot>) -> usize {
      let size = syst.boot_services().memory_map_size();
      let mut buffer = alloc::vec![0; size];
      let (_key, mmap) = syst
            .boot_services()
            .memory_map(&mut buffer)
            .expect_success("Failed to get the memory map");

      let mut addr_max = 0;

      for block in mmap {
            addr_max = core::cmp::max(
                  addr_max,
                  block.phys_start + (block.page_count << paging::PAGE_SHIFT),
            );
      }
      assert!(addr_max > 0);

      let pf_buffer_size = PF_SIZE * (addr_max as usize).div_ceil_bit(paging::PAGE_SHIFT);
      let pf_buffer = alloc(syst)
            .alloc_n(pf_buffer_size >> paging::PAGE_SHIFT)
            .expect("Failed to allocate the page frame buffer");

      let pf_virt = paging::LAddr::from(KMEM_PHYS_BASE)
            ..paging::LAddr::from(KMEM_PHYS_BASE + pf_buffer_size);
      maps(syst, pf_virt, pf_buffer, paging::Attr::KERNEL_RWNE).expect("Failed to map page frames");

      {
            let phys = paging::PAddr::new(0);
            let virt = paging::LAddr::from(KERNEL_ID_OFFSET)
                  ..paging::LAddr::from(KERNEL_ID_OFFSET + addr_max as usize);
            maps(syst, virt, phys, paging::Attr::KERNEL_RWNE)
                  .expect("Failed to map physical pages identically");
      }

      size
}

pub fn commit_mapping() {
      unsafe {
            let mut efer = archop::msr::read(archop::msr::MSR::EFER);
            efer |= 1 << 11;
            archop::msr::write(archop::msr::MSR::EFER, efer);

            let cr3 = ROOT_TABLE.assume_init();
            asm!("mov cr3, {}", in(reg) cr3.as_mut_ptr());
      }
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

// pub fn config_efi_runtime<'a>(
//       rt: &SystemTable<Runtime>,
//       mmap: impl ExactSizeIterator<Item = &'a boot::MemoryDescriptor>,
// ) {
//       for block in mmap {
            
//       }
// }
