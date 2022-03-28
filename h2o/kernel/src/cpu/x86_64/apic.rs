pub mod ipi;
pub mod timer;

use alloc::collections::BTreeMap;
use core::arch::asm;

use archop::{msr, Azy};
use modular_bitfield::prelude::*;
use paging::{LAddr, PAddr, PAGE_SIZE};
use raw_cpuid::CpuId;
use spin::RwLock;

use super::intr::def::ApicVec;
use crate::mem::space::{self, Flags, Phys};

pub static LAPIC_ID: RwLock<BTreeMap<usize, u32>> = RwLock::new(BTreeMap::new());
static LAPIC_BASE: Azy<usize> = Azy::new(|| {
    let phys = Phys::new(PAddr::new(0xFEE00000), PAGE_SIZE).expect("Failed to acquire LAPIC base");
    space::KRL
        .map(
            None,
            Phys::clone(&phys),
            0,
            space::page_aligned(phys.len()),
            Flags::READABLE | Flags::WRITABLE | Flags::UNCACHED,
        )
        .expect("Failed to allocate memory")
        .val()
});
#[thread_local]
static mut LAPIC: Option<Lapic> = None;

/// Get the per-CPU instance of Local APIC.
pub unsafe fn lapic<F, R>(f: F) -> R
where
    F: FnOnce(&'static mut Lapic) -> R,
{
    f(LAPIC.as_mut().expect("Local APIC uninitialized"))
}

pub enum LapicType {
    X1(LAddr),
    X2,
}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
#[repr(u64)]
#[bits = 3]
pub enum DelivMode {
    Fixed = 0b000,
    LowestPriority = 0b001,
    Smi = 0b010,
    Nmi = 0b100,
    Init = 0b101,
    StartUp = 0b110,
    ExtInt = 0b111,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BitfieldSpecifier)]
#[repr(u64)]
pub enum Polarity {
    High = 0,
    Low = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BitfieldSpecifier)]
#[repr(u64)]
pub enum TriggerMode {
    Edge = 0,
    Level = 1,
}

#[derive(Clone, Copy)]
#[bitfield]
struct LocalEntry {
    #[skip(getters)]
    vec: u8,
    #[bits = 3]
    #[skip(getters)]
    deliv_mode: DelivMode,
    #[skip]
    __: B1,
    #[skip(getters)]
    pending: bool,
    #[skip(getters)]
    #[bits = 1]
    polarity: Polarity,
    #[skip(getters)]
    remote_irr: bool,
    #[skip(getters)]
    #[bits = 1]
    trigger_mode: TriggerMode,
    #[skip(getters)]
    mask: bool,
    #[skip(getters)]
    timer_mode: timer::TimerMode,
    #[skip]
    __: B13,
}

impl From<u32> for LocalEntry {
    fn from(x: u32) -> Self {
        Self::from_bytes(x.to_ne_bytes())
    }
}

impl From<LocalEntry> for u32 {
    fn from(x: LocalEntry) -> Self {
        Self::from_ne_bytes(x.into_bytes())
    }
}

pub struct Lapic {
    ty: LapicType,
    id: u32,
}

impl Lapic {
    #[inline]
    fn reg_32_to_1_off(reg: msr::Msr) -> usize {
        (reg as u32 as usize - 0x800) << 4
    }

    #[inline]
    fn reg_64_to_1_off(reg: msr::Msr) -> (usize, usize) {
        let r0 = Self::reg_32_to_1_off(reg);
        (r0, r0 + 0x10)
    }

    #[inline]
    unsafe fn base_to_ptr(base: LAddr, offset: usize) -> *mut u32 {
        base.add(offset).cast::<u32>()
    }

    unsafe fn read_reg_32(ty: &mut LapicType, reg: msr::Msr) -> u32 {
        match ty {
            LapicType::X1(base) => {
                let ptr = Self::base_to_ptr(*base, Self::reg_32_to_1_off(reg));
                ptr.read_volatile()
            }
            LapicType::X2 => msr::read(reg) as u32,
        }
    }

    unsafe fn write_reg_32(ty: &mut LapicType, reg: msr::Msr, val: u32) {
        match ty {
            LapicType::X1(base) => {
                let ptr = Self::base_to_ptr(*base, Self::reg_32_to_1_off(reg));
                ptr.write_volatile(val)
            }
            LapicType::X2 => msr::write(reg, val as u64),
        }
    }

    #[allow(dead_code)]
    unsafe fn read_reg_64(ty: &mut LapicType, reg: msr::Msr) -> u64 {
        match ty {
            LapicType::X1(base) => {
                let (lp, hp) = Self::reg_64_to_1_off(reg);
                let low = Self::base_to_ptr(*base, lp).read_volatile() as u64;
                let high = Self::base_to_ptr(*base, hp).read_volatile() as u64;
                low | (high << 32)
            }
            LapicType::X2 => msr::read(reg),
        }
    }

    unsafe fn write_reg_64(ty: &mut LapicType, reg: msr::Msr, val: u64) {
        match ty {
            LapicType::X1(base) => {
                let (low, high) = ((val & 0xFFFFFFFF) as u32, ((val >> 32) as u32));
                let (lp, hp) = Self::reg_64_to_1_off(reg);
                // !!: The order of writing must be from high to low.
                Self::base_to_ptr(*base, hp).write_volatile(high);
                Self::base_to_ptr(*base, lp).write_volatile(low);
            }
            LapicType::X2 => msr::write(reg, val),
        }
    }

    unsafe fn clear_ixr(&mut self) {
        let cnt = (0..8).fold(0, |acc, i| {
            acc + Self::read_reg_32(
                &mut self.ty,
                core::mem::transmute(msr::X2APIC_ISR0 as u32 + i),
            )
            .count_ones()
        });
        for _ in 0..cnt {
            self.eoi();
        }
    }

    pub fn new() -> Self {
        let mut ty = {
            let has_x2apic = CpuId::new()
                .get_feature_info()
                .map_or(false, |f| f.has_x2apic());
            if has_x2apic {
                // SAFETY: Enabling Local X2 APIC if possible.
                unsafe {
                    let val = msr::read(msr::APIC_BASE);
                    msr::write(msr::APIC_BASE, val | (1 << 10));
                }
                LapicType::X2
            } else {
                LapicType::X1(LAddr::from(*LAPIC_BASE))
            }
        };

        // Get the LAPIC ID.
        let mut id = unsafe { Self::read_reg_32(&mut ty, msr::X2APICID) };
        if let LapicType::X2 = &ty {
            id >>= 24;
        }
        LAPIC_ID.write().insert(unsafe { crate::cpu::id() }, id);

        let mut lapic = Lapic { ty, id };

        unsafe {
            lapic.clear_ixr();

            // Accept all the interrupt vectors but `0..32` since they are reserved by
            // exceptions.
            unsafe { Self::write_reg_32(&mut lapic.ty, msr::X2APIC_TPR, 0x10) };

            let lint0 = LocalEntry::new()
                .with_deliv_mode(DelivMode::ExtInt)
                .with_mask(true);
            Self::write_reg_32(&mut lapic.ty, msr::X2APIC_LVT_LINT0, lint0.into());

            // The NMI interrupt is on LINT1 and only BSP accepts NMI.
            let lint1 = LocalEntry::new()
                .with_deliv_mode(DelivMode::Nmi)
                .with_mask(crate::cpu::id() != 0)
                .with_trigger_mode(TriggerMode::Level);
            Self::write_reg_32(&mut lapic.ty, msr::X2APIC_LVT_LINT1, lint1.into());

            let lerr = LocalEntry::new().with_vec(ApicVec::Error as u8);
            Self::write_reg_32(&mut lapic.ty, msr::X2APIC_LVT_ERROR, lerr.into());
        }

        lapic
    }

    /// # Safety
    ///
    /// WARNING: This function modifies the architecture's basic registers. Be
    /// sure to make preparations.
    pub unsafe fn enable(&mut self) {
        Self::write_reg_32(
            &mut self.ty,
            msr::X2APIC_SIVR,
            (1 << 8) | (super::intr::def::ApicVec::Spurious as u32),
        );
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    /// # Safety
    ///
    /// WARNING: This function modifies the architecture's basic registers. Be
    /// sure to make preparations.
    pub unsafe fn eoi(&mut self) {
        Self::write_reg_32(&mut self.ty, msr::X2APIC_EOI, 0)
    }

    /// # Safety
    ///
    /// WARNING: This function modifies the architecture's basic registers. Be
    /// sure to make preparations.
    ///
    /// The caller must ensure that IDT is initialized before LAPIC Timer's
    /// activation and that `div` is within the range [`timer::DIV`].
    pub unsafe fn activate_timer(&mut self, mode: timer::TimerMode, div: u8, init_value: u64) {
        timer::activate(self, mode, div, init_value);
    }

    /// # Safety
    ///
    /// The caller must ensure that this function is only called by
    /// [`error_handler`].
    pub(self) unsafe fn handle_error(&mut self) {
        let esr = Self::read_reg_32(&mut self.ty, msr::X2APIC_ESR);
        self.eoi();

        const MAX_ERROR: usize = 8;
        const ERROR_MSG: [&str; MAX_ERROR] = [
            "Send CS error",            // APIC Error Bit 0
            "Receive CS error",         // APIC Error Bit 1
            "Send accept error",        // APIC Error Bit 2
            "Receive accept error",     // APIC Error Bit 3
            "Redirectable IPI",         // APIC Error Bit 4
            "Send illegal vector",      // APIC Error Bit 5
            "Received illegal vector",  // APIC Error Bit 6
            "Illegal register address", // APIC Error Bit 7
        ];

        log::error!("Local APIC ERROR:");

        let mut it = esr;
        for error_msg in ERROR_MSG.iter() {
            if (it & 1) != 0 {
                log::error!("> {}", error_msg);
            }
            it >>= 1;
        }
    }

    /// # Safety
    ///
    /// The caller must ensure that `dest_apicid` and `shorthand` corresponds
    /// with each other and `vec` is valid.
    ///
    /// WARNING: This function modifies the architecture's basic registers. Be
    /// sure to make preparations.
    pub unsafe fn send_ipi(
        &mut self,
        vec: u8,
        deliv_mode: DelivMode,
        shorthand: ipi::Shorthand,
        dest_apicid: u32,
    ) {
        let icr_low = ipi::IcrEntry::new()
            .with_vec(vec)
            .with_deliv_mode(deliv_mode)
            .with_shorthand(shorthand);
        let icr_high = match self.ty {
            LapicType::X1(_) => dest_apicid << 24,
            LapicType::X2 => dest_apicid,
        };
        Self::write_reg_64(
            &mut self.ty,
            msr::X2APIC_ICR,
            u32::from(icr_low) as u64 | ((icr_high as u64) << 32),
        );
    }
}

/// # Safety
///
/// The caller must ensure that this function is only called by the spurious
/// handler.
pub unsafe fn spurious_handler() {
    asm!("nop", options(nomem, nostack));
}

/// # Safety
///
/// The caller must ensure that this function is only called by the error
/// handler.
pub unsafe fn error_handler() {
    // SAFETY: Inside the interrupt error handler.
    lapic(|lapic| lapic.handle_error());
}

pub unsafe fn init() {
    let mut lapic = Lapic::new();
    lapic.enable();
    lapic.activate_timer(timer::TimerMode::Periodic, 7, 512);

    LAPIC = Some(lapic);
}
