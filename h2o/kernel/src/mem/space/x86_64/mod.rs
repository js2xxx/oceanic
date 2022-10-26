//! # Memory management of H2O in x86_64.
//!
//! This module is specific for x86_64 mode. It wraps the cr3's root page table
//! and the methods of x86_64 paging.

use alloc::{alloc::Global, boxed::Box};
use core::{alloc::Allocator, ops::Range};

use archop::Azy;
use canary::Canary;
use paging::{LAddr, PAddr, Table};
use spin::Mutex;

use super::Flags;
use crate::sched::{task::ctx::x86_64::Frame, SCHED};

/// The root page table at initialization time.
static KERNEL_ROOT: Azy<(Box<Table>, u64)> = Azy::new(|| {
    let mut table = box Table::zeroed();

    let cr3 = unsafe { archop::reg::cr3::read() };
    let cr3_laddr = PAddr::new(cr3 as usize).to_laddr(minfo::ID_OFFSET);
    let init_table = unsafe { core::slice::from_raw_parts(cr3_laddr.cast(), paging::NR_ENTRIES) };
    table.copy_from_slice(init_table);

    (table, cr3)
});

pub fn init_pgc() -> u64 {
    KERNEL_ROOT.1
}

/// The root page table.
#[derive(Debug)]
pub struct Space {
    canary: Canary<Space>,
    root_table: Mutex<Box<Table>>,
    cr3: PAddr,
}

impl Space {
    #[inline]
    fn flags_to_pg_attr(flags: Flags) -> paging::Attr {
        let uncached = flags.contains(Flags::UNCACHED);
        paging::Attr::builder()
            .writable(flags.contains(Flags::WRITABLE))
            .user_access(flags.contains(Flags::USER_ACCESS))
            .executable(flags.contains(Flags::EXECUTABLE))
            .cache(uncached, uncached)
            .build()
    }

    fn pg_attr_to_flags(attr: paging::Attr) -> Flags {
        let mut flags = Flags::READABLE;
        if attr.contains(paging::Attr::WRITABLE) {
            flags |= Flags::WRITABLE;
        }
        if attr.contains(paging::Attr::USER_ACCESS) {
            flags |= Flags::USER_ACCESS;
        }
        if !attr.contains(paging::Attr::EXE_DISABLE) {
            flags |= Flags::EXECUTABLE;
        }
        if attr.contains(paging::Attr::CACHE_DISABLE) {
            flags |= Flags::UNCACHED;
        }
        flags
    }

    /// Construct a new arch-specific space.
    ///
    /// The space's root page table must contains the page tables of the kernel
    /// half otherwise if loaded the kernel will crash due to #PF.
    pub fn new() -> Space {
        let rt = box Table::zeroed();
        let cr3 = Box::into_raw(rt);

        let space = Space {
            canary: Canary::new(),
            root_table: Mutex::new(unsafe { Box::from_raw(cr3) }),
            cr3: LAddr::new(cr3.cast()).to_paddr(minfo::ID_OFFSET),
        };

        {
            // Only copying the higher half kernel mappings.
            let mut dst_rt = space.root_table.lock();
            let dst_kernel_half = &mut dst_rt[(paging::NR_ENTRIES / 2)..];

            let src_kernel_half = &KERNEL_ROOT.0[(paging::NR_ENTRIES / 2)..];

            dst_kernel_half.copy_from_slice(src_kernel_half);
        }

        space
    }

    pub(in crate::mem) fn maps(
        &self,
        virt: Range<LAddr>,
        phys: PAddr,
        flags: Flags,
    ) -> Result<(), paging::Error> {
        self.canary.assert();

        let map_info = paging::MapInfo {
            virt,
            phys,
            attr: Self::flags_to_pg_attr(flags),
            id_off: minfo::ID_OFFSET,
        };

        paging::maps(&mut self.root_table.lock(), &map_info, &mut PageAlloc)
    }

    pub(in crate::mem) fn reprotect(
        &self,
        virt: Range<LAddr>,
        flags: Flags,
    ) -> Result<(), paging::Error> {
        self.canary.assert();

        let reprotect_info = paging::ReprotectInfo {
            virt,
            attr: Self::flags_to_pg_attr(flags),
            id_off: minfo::ID_OFFSET,
        };

        paging::reprotect(&mut self.root_table.lock(), &reprotect_info, &mut PageAlloc)
    }

    #[allow(dead_code)]
    pub(in crate::mem) fn query(&self, virt: LAddr) -> Result<(PAddr, Flags), paging::Error> {
        self.canary.assert();

        paging::query(&self.root_table.lock(), virt, minfo::ID_OFFSET)
            .map(|(phys, attr)| (phys, Self::pg_attr_to_flags(attr)))
    }

    pub(in crate::mem) fn unmaps(
        &self,
        virt: Range<LAddr>,
    ) -> Result<Option<PAddr>, paging::Error> {
        self.canary.assert();

        let mut lck = self.root_table.lock();
        let phys = paging::query(&lck, virt.start, minfo::ID_OFFSET)
            .ok()
            .map(|(phys, _)| phys);
        paging::unmaps(&mut lck, virt, minfo::ID_OFFSET, &mut PageAlloc).map(|_| phys)
    }

    /// # Safety
    ///
    /// The caller must ensure that loading the space is safe and not cause any
    /// #PF.
    pub(in crate::mem) unsafe fn load(&self) {
        archop::reg::cr3::write(*self.cr3 as u64);
    }
}

impl PartialEq for Space {
    fn eq(&self, other: &Self) -> bool {
        self.cr3 == other.cr3
    }
}

impl Eq for Space {}

struct PageAlloc;

unsafe impl paging::PageAlloc for PageAlloc {
    unsafe fn allocate(&mut self) -> Option<PAddr> {
        Global
            .allocate(core::alloc::Layout::new::<paging::Table>())
            .map_or(None, |ptr| {
                Some(LAddr::new(ptr.as_mut_ptr()).to_paddr(minfo::ID_OFFSET))
            })
    }

    unsafe fn deallocate(&mut self, addr: PAddr) {
        if let Some(ptr) = addr.to_laddr(minfo::ID_OFFSET).as_non_null() {
            Global.deallocate(ptr, core::alloc::Layout::new::<paging::Table>());
        }
    }
}

bitflags::bitflags! {
    #[repr(C)]
    pub struct ErrCode: u64 {
        const PRESENT      = 0b0000001;
        const WRITE        = 0b0000010;
        const USER_ACCESS  = 0b0000100;
        const RESERVED     = 0b0001000;
        const EXECUTING    = 0b0010000;
        const PROT_KEY     = 0b0100000;
        const SHADOW_STACK = 0b1000000;
        const SGX = 1 << 15;
    }
}

impl ErrCode {
    pub const FMT: &'static str = "P WR US RSVD ID PK SS - - - - - - - - SGX";
}

pub unsafe fn page_fault(frame: &mut Frame, errc: u64) -> bool {
    let addr = archop::reg::cr2::read();

    match ErrCode::from_bits(errc) {
        // So far neither has been supported.
        Some(code) if !code.contains(ErrCode::PROT_KEY | ErrCode::SHADOW_STACK) => {
            if SCHED
                .with_current(|cur| cur.kstack_mut().pf_resume(frame, errc, addr))
                .is_ok()
            {
                return true;
            }

            // TODO: Add some handling code.
        }
        _ => {}
    }

    // No more available remedies.
    false
}
