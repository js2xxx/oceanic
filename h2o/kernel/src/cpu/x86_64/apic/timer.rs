use core::ops::Range;

use modular_bitfield::prelude::*;

use super::LocalEntry;
use crate::cpu::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, BitfieldSpecifier)]
#[repr(u32)]
#[bits = 2]
pub enum TimerMode {
    OneShot = 0,
    Periodic = 1,
    TscDeadline = 2,
}

pub const DIV: Range<u8> = 0..8;

/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure
/// to make preparations.
///
/// The caller must ensure that IDT is initialized before LAPIC Timer's
/// activation and that `div` is within the range [`DIV`].
pub unsafe fn activate(lapic: &mut super::Lapic, mode: TimerMode, div: u8, init_value: u32) {
    /// # Safety
    ///
    /// The caller must ensure that `div` is within the range [`DIV`].
    unsafe fn encode_div(div: u8) -> u8 {
        let t = (div + 7) & 7;
        (t & 0x3) | ((t & 0x4) << 1)
    }

    let vec = crate::cpu::intr::arch::def::ApicVec::Timer as u8;

    // SAFETY: `div` is valid.
    let encdiv = unsafe { encode_div(div) };
    let timer_val = LocalEntry::new().with_timer_mode(mode).with_vec(vec);

    // SAFETY: Those MSRs are per-cpu and only 1 timer object is available in the
    // context.
    unsafe {
        use archop::msr;

        use super::Lapic;
        Lapic::write_reg_32(&mut lapic.ty, msr::X2APIC_DIV_CONF, encdiv.into());
        Lapic::write_reg_32(&mut lapic.ty, msr::X2APIC_LVT_TIMER, timer_val.into());
        if matches!(mode, TimerMode::TscDeadline) {
            msr::write(msr::TSC_DEADLINE, init_value.into());
        } else {
            Lapic::write_reg_32(&mut lapic.ty, msr::X2APIC_INIT_COUNT, init_value);
        }
    }
}

/// # Safety
///
/// The caller must ensure that this function is called only by interrupt
/// routines and when everything about interrupts is set up.
pub unsafe fn timer_handler() {
    // SAFETY: Inside the timer interrupt handler.
    super::lapic(|lapic| lapic.eoi());

    crate::cpu::time::timer_tick();
    crate::sched::SCHED.tick(Instant::now());
}
