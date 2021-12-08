use core::{mem::MaybeUninit, ops::Range, ptr::NonNull};

use bitop_ex::BitOpEx;
use minfo::{ID_OFFSET as KERNEL_ID_OFFSET, INITIAL_ID_SPACE, KMEM_PHYS_BASE, PF_SIZE};
use paging::PageAlloc;
use uefi::{prelude::*, table::boot};

pub const EFI_ID_OFFSET: usize = 0;
static mut ROOT_TABLE: MaybeUninit<NonNull<paging::Table>> = MaybeUninit::uninit();

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
        let ret = self
            .bs
            .allocate_pages(
                boot::AllocateType::AnyPages,
                boot::MemoryType::LOADER_DATA,
                n,
            )
            .ok()
            .map(|c| paging::PAddr::new(c.log() as usize));
        if let Some(ret) = ret {
            log::trace!("allocated {:x} ~ {:x}", *ret, *ret + n * paging::PAGE_SIZE);
        }
        ret
    }

    pub fn dealloc_n(&mut self, phys: paging::PAddr, n: usize) {
        log::trace!(
            "deallocated {:x} ~ {:x}",
            *phys,
            *phys + n * paging::PAGE_SIZE
        );
        let _ = self.bs.free_pages(*phys as u64, n).log_warning();
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

    pub fn dealloc_from_slice(&mut self, slice: *mut [u8], id_off: usize) {
        let phys = paging::LAddr::new(slice.cast()).to_paddr(id_off);
        let n = slice.len().div_ceil_bit(paging::PAGE_SHIFT);
        self.dealloc_n(phys, n)
    }
}

unsafe impl<'a> paging::PageAlloc for BootAlloc<'a> {
    unsafe fn allocate(&mut self) -> Option<paging::PAddr> {
        self.alloc_n(1)
    }

    unsafe fn deallocate(&mut self, addr: paging::PAddr) {
        self.dealloc_n(addr, 1)
    }
}

pub fn init(syst: &SystemTable<Boot>) {
    log::trace!("mem::init: syst = {:?}", syst as *const _);

    let rt_addr =
        unsafe { alloc(syst).allocate_zeroed(EFI_ID_OFFSET) }.expect("Failed to allocate a page");
    let mut rt = unsafe { NonNull::new_unchecked(*rt_addr as *mut paging::Table) };

    unsafe { ROOT_TABLE.as_mut_ptr().write(rt) };

    let phys = paging::PAddr::new(0);
    let virt_efi =
        paging::LAddr::from(EFI_ID_OFFSET)..paging::LAddr::from(INITIAL_ID_SPACE + EFI_ID_OFFSET);
    let pg_attr = paging::Attr::KERNEL_RW;

    for i in (paging::NR_ENTRIES / 2)..paging::NR_ENTRIES {
        let phys = unsafe { alloc(syst).allocate_zeroed(EFI_ID_OFFSET) }
            .expect("Failed to allocate a page");
        let attr = paging::Attr::INTERMEDIATE;
        unsafe { rt.as_mut()[i] = paging::Entry::new(phys, attr, paging::Level::Pt) };
    }

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
        unsafe { ROOT_TABLE.assume_init().as_mut() },
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
        unsafe { ROOT_TABLE.assume_init().as_mut() },
        virt,
        EFI_ID_OFFSET,
        &mut BootAlloc {
            bs: &syst.boot_services(),
        },
    )
}

pub fn init_pf(syst: &SystemTable<Boot>) -> (usize, usize) {
    let mmap_size = syst.boot_services().memory_map_size();
    let mut buffer = alloc::vec![0; mmap_size * 6 / 5];
    let (_key, mmap) = syst
        .boot_services()
        .memory_map(&mut buffer)
        .expect_success("Failed to get the memory map");

    // Get the size of the memory map entries.
    let (entry_size, addr_max) = {
        let mut addr_max = 0;
        let mut last = None::<*const u8>;
        let mut size = None;
        for block in mmap {
            addr_max = addr_max.max(block.phys_start + (block.page_count << paging::PAGE_SHIFT));

            let cur = block as *const boot::MemoryDescriptor as *const u8;
            size = size.or(last.map(|last| unsafe { last.cast::<u8>().offset_from(cur).abs() }));
            last.get_or_insert(cur);
        }
        debug_assert!(addr_max > 0);
        (size.unwrap().abs_diff(0), addr_max)
    };

    // Allocate memory for the PMM.
    let (pf_virt, pf_phys) = {
        let size = (PF_SIZE * (addr_max as usize).div_ceil_bit(paging::PAGE_SHIFT))
            .round_up_bit(paging::PAGE_SHIFT);
        let buffer = alloc(syst)
            .alloc_n(size >> paging::PAGE_SHIFT)
            .expect("Failed to allocate the page frame buffer");

        // Clear the physical memory area for the PMM in case of dirty bytes.
        unsafe {
            let pf_ptr = buffer.to_laddr(EFI_ID_OFFSET);
            pf_ptr.write_bytes(0, size);
        }

        let virt = paging::LAddr::from(KMEM_PHYS_BASE)..paging::LAddr::from(KMEM_PHYS_BASE + size);
        (virt, buffer)
    };
    maps(syst, pf_virt, pf_phys, paging::Attr::KERNEL_RWNE).expect("Failed to map page frames");

    // Create the identical mapping for the kernel.
    {
        let phys = paging::PAddr::new(0);
        let virt = paging::LAddr::from(KERNEL_ID_OFFSET)
            ..paging::LAddr::from(KERNEL_ID_OFFSET + addr_max as usize);
        maps(syst, virt, phys, paging::Attr::KERNEL_RWNE)
            .expect("Failed to map physical pages identically");
    }

    (entry_size, mmap_size)
}

pub fn commit_mapping() {
    use archop::msr;
    unsafe {
        let mut efer = msr::read(msr::EFER);
        efer |= 1 << 11;
        msr::write(msr::EFER, efer);

        let cr3 = ROOT_TABLE.assume_init();
        asm!("mov cr3, {}", in(reg) cr3.as_ptr());
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
