use super::LocalEntry;

use core::ops::Range;
use modular_bitfield::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, BitfieldSpecifier)]
#[repr(u32)]
#[bits = 2]
pub enum TimerMode {
      OneShot = 0,
      Periodic = 1,
      TscDeadline = 2,
}

pub const DIV: Range<u8> = 0..8;

pub struct Timer<'a> {
      mode: TimerMode,
      div: u8,
      lapic: super::Lapic<'a>,
}

impl<'a> Timer<'a> {
      pub(super) fn new(mode: TimerMode, div: u8, lapic: super::Lapic<'a>) -> Timer<'a> {
            Timer { mode, div, lapic }
      }

      // pub fn mode(&mut self, mode: TimerMode) -> &'a mut Timer {
      //       self.mode = mode;
      //       self
      // }

      // /// # Safety
      // ///
      // /// The caller must ensure that `div` is within the range [`DIV`].
      // pub unsafe fn div_unchecked(&mut self, div: u8) -> &'a mut Timer {
      //       self.div = div;
      //       self
      // }

      // pub fn div(&mut self, div: u8) -> Result<&'a mut Timer, Range<u8>> {
      //       if DIV.contains(&div) {
      //             // SAFE: `div` has been checked above.
      //             Ok(unsafe { self.div_unchecked(div) })
      //       } else {
      //             Err(DIV.clone())
      //       }
      // }

      /// # Safety
      ///
      /// The caller must ensure that `div` is within the range [`DIV`].
      unsafe fn encode_div(div: u8) -> u8 {
            let t = (div + 7) & 7;
            (t & 0x3) | ((t & 0x4) << 1)
      }

      /// # Safety
      ///
      /// WARNING: This function modifies the architecture's basic registers. Be sure to make
      /// preparations.
      ///
      /// The caller must ensure that IDT is initialized before LAPIC Timer's activation.
      pub unsafe fn activate(self, init_value: u64) -> (super::Lapic<'a>, TimerMode, u8) {
            let Timer {
                  mode,
                  div,
                  mut lapic,
            } = self;
            let vec = crate::cpu::intr::arch::def::ApicVec::Timer as u8;

            // SAFE: `div` is valid.
            let encdiv = unsafe { Self::encode_div(div) };
            let timer_val = LocalEntry::new().with_timer_mode(mode).with_vec(vec);

            // SAFE: Those MSRs are per-cpu and only 1 timer object is available in the context.
            unsafe {
                  use super::Lapic;
                  use archop::msr;
                  Lapic::write_reg_32(&mut lapic.ty, msr::X2APIC_DIV_CONF, encdiv.into());
                  Lapic::write_reg_32(&mut lapic.ty, msr::X2APIC_LVT_TIMER, timer_val.into());
                  if let TimerMode::TscDeadline = mode {
                        msr::write(msr::TSC_DEADLINE, init_value);
                  } else {
                        Lapic::write_reg_64(&mut lapic.ty, msr::X2APIC_INIT_COUNT, init_value);
                  }
            }

            (lapic, mode, div)
      }
}

/// # Safety
///
/// The caller must ensure that this function is called only by interrupt routines and when
/// everything about interrupts is set up.
pub unsafe fn timer_handler(_frame: *mut crate::cpu::intr::arch::ctx::Frame) {
      // SAFE: Inside the timer interrupt handler.
      let kernel_gs = unsafe { crate::cpu::arch::KernelGs::access_in_intr() };
      let lapic = &mut kernel_gs.lapic;
      lapic.eoi();
}
