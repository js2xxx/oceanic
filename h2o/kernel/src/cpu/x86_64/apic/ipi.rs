use alloc::vec::Vec;
use core::{
    cell::UnsafeCell,
    ptr::null_mut,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use modular_bitfield::prelude::*;
use paging::PAddr;

use super::{DelivMode, TriggerMode};
use crate::{
    cpu::{
        arch::{
            apic::{ipi, lapic},
            intr,
            seg::{alloc_pls, ndt::Segment},
        },
        time::{delay, Instant},
    },
    dev::acpi::table::madt::LapicNode,
    mem::space::init_pgc,
};

// pub fn ipi_handler() {}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
#[repr(u64)]
pub enum Shorthand {
    None = 0,
    Me = 1,
    All = 2,
    Others = 3,
}

#[derive(Clone, Copy)]
#[bitfield]
pub struct IcrEntry {
    pub(super) vec: u8,
    #[bits = 3]
    pub(super) deliv_mode: DelivMode,
    pub(super) dest_logical: bool,
    pub(super) pending: bool,
    #[skip]
    __: B1,
    pub(super) level_assert: bool,
    #[bits = 1]
    pub(super) trigger_mode: TriggerMode,
    #[skip]
    __: B2,
    pub(super) shorthand: Shorthand,
    #[skip]
    __: B12,
}

impl From<u32> for IcrEntry {
    fn from(x: u32) -> Self {
        Self::from_bytes(x.to_ne_bytes())
    }
}

impl From<IcrEntry> for u32 {
    fn from(x: IcrEntry) -> Self {
        Self::from_ne_bytes(x.into_bytes())
    }
}

#[repr(C)]
pub struct TramSubheader {
    stack: u64,
    pls: u64,
}

#[repr(C)]
pub struct TramHeader {
    booted: AtomicBool,
    subheader: UnsafeCell<TramSubheader>,
    pgc: u64,
    kmain: *mut u8,
    init_efer: u64,
    init_cr4: u64,
    init_cr0: u64,
    gdt: [Segment; 3],
}

impl TramHeader {
    pub unsafe fn new() -> TramHeader {
        use archop::{msr, reg};

        TramHeader {
            booted: AtomicBool::new(true),
            subheader: UnsafeCell::new(core::mem::zeroed()),
            pgc: init_pgc(),
            kmain: crate::kmain_ap as *mut _,
            init_efer: msr::read(msr::EFER),
            init_cr4: reg::cr4::read(),
            init_cr0: reg::cr0::read(),
            gdt: {
                use crate::cpu::arch::seg::attrs;
                const LIM: u32 = 0xFFFFF;
                const ATTR: u16 = attrs::PRESENT | attrs::G4K;

                [
                    Segment::new(0, 0, 0, 0),
                    Segment::new(0, LIM, attrs::SEG_CODE | attrs::X64 | ATTR, 0),
                    Segment::new(0, LIM, attrs::SEG_DATA | attrs::X64 | ATTR, 0),
                ]
            },
        }
    }

    pub unsafe fn test_booted(&self) -> bool {
        let limit = Duration::from_millis(50);
        let instant = Instant::now();
        while !self.booted.swap(false, Ordering::SeqCst) && instant.elapsed() < limit {
            core::hint::spin_loop();
        }
        instant.elapsed() < limit
    }

    pub unsafe fn reset_subheader(&self) {
        let stack = crate::mem::alloc_system_stack()
            .expect("System memory allocation failed")
            .as_ptr() as u64;

        let pls = alloc_pls().map_or(null_mut(), |ptr| ptr.as_ptr()) as u64;

        let ptr = self.subheader.get();
        ptr.write(TramSubheader { stack, pls });
    }
}

/// # Safety
///
/// This function must be called after Local APIC initialization.
pub unsafe fn start_cpus(lapics: Vec<LapicNode>) -> usize {
    static TRAM_DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/tram"));

    let base_phys = PAddr::new(minfo::TRAMPOLINE_RANGE.start);
    let base = base_phys.to_laddr(minfo::ID_OFFSET);

    let ptr = *base;

    unsafe {
        let slice = core::slice::from_raw_parts_mut(ptr, TRAM_DATA.len());
        slice.copy_from_slice(TRAM_DATA);
    }

    let header = {
        let header = ptr.add(16).cast::<TramHeader>();
        unsafe { header.write(TramHeader::new()) };
        &*header
    };

    let mut cnt = lapics.len();

    let self_id = lapic(|lapic| lapic.id);
    for LapicNode { id, .. } in lapics.iter().filter(|node| node.id != self_id) {
        header.reset_subheader();

        lapic(|lapic| {
            lapic.send_ipi(0, DelivMode::Init, ipi::Shorthand::None, *id);
            delay(Duration::from_millis(50));

            lapic.send_ipi(
                (*base_phys >> 3) as u8,
                DelivMode::StartUp,
                ipi::Shorthand::None,
                *id,
            );

            if !header.test_booted() {
                lapic.send_ipi(
                    (*base_phys >> 3) as u8,
                    DelivMode::StartUp,
                    ipi::Shorthand::None,
                    *id,
                );

                if !header.test_booted() {
                    log::warn!("CPU with LAPIC ID {} failed to boot", id);
                    cnt -= 1;
                }
            }
        });
    }

    cnt
}

/// # Safety
///
/// This function must be called only by the scheduler of the current CPU and
/// the caller must ensure that `cpu` is valid.
pub unsafe fn task_migrate(cpu: usize) {
    lapic(|lapic| {
        match super::LAPIC_ID.read().get(&cpu) {
            Some(&id) => lapic.send_ipi(
                intr::def::ApicVec::IpiTaskMigrate as u8,
                DelivMode::Fixed,
                Shorthand::None,
                id,
            ),
            None => log::warn!("CPU #{} not present", cpu),
        };
    });
}
