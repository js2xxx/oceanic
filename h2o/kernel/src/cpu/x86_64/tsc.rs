use core::time::Duration;

use archop::io::{Io, Port};
use archop::msr::rdtsc;

use static_assertions::const_assert;

static mut TSC_FREQ_KHZ: u64 = 2600000;
static mut TSC_INITIAL: u64 = 0;
static mut NS_CLOCK_FACTORS: (u64, u64) = (0, 0);

/// Get the per-CPU clock in nanoseconds.
///
/// # Safety
///
/// This function must be called only after [`tsc_init`].
pub unsafe fn ns_clock() -> u64 {
      let val = rdtsc() - TSC_INITIAL;
      let (mul, sft) = NS_CLOCK_FACTORS;
      (((val & 0xFFFFFFFF) * mul) >> sft) | (((val >> 32) * mul) << (32 - sft))
}

/// Delay an amount of time in nanoseconds.
///
/// # Safety
///
/// This function must be called only after [`tsc_init`].
pub unsafe fn delay(duration: Duration) {
      let ns = duration.as_nanos() as u64;
      let start = ns_clock();
      while ns_clock() - start < ns {}
}

/// Calibrate the CPU's frequency (KHz) by activating the PIT timer.
unsafe fn pit_calibrate_tsc() -> u64 {
      const PIT_RATE: u64 = 1193182;
      const CALIB_TIME_MS: u64 = 50;
      const CALIB_LATCH: u64 = PIT_RATE / (1000 / CALIB_TIME_MS);
      const_assert!(CALIB_LATCH <= core::u16::MAX as u64);

      let mut speaker = Port::<u8>::new(0x61);
      // Set the Gate high and disable speaker
      speaker.write((speaker.read() & !0x02) | 0x01);

      let mut pit = Port::<u8>::new(0x40);
      // Channel 2, mode 0 (one-shot), binary count
      pit.write_offset(3, 0xb0);

      pit.write_offset(2, (CALIB_LATCH & 0xff) as u8);
      pit.write_offset(2, (CALIB_LATCH >> 8) as u8);

      let mut t = 0;
      let start = rdtsc();
      while (speaker.read() & 0x20) == 0 {
            t = rdtsc();
      }
      let end = t;

      (end - start) / CALIB_TIME_MS
}

/// # Safety
///
/// This function must be called only once from the bootstrap CPU.
pub unsafe fn init() {
      TSC_FREQ_KHZ = pit_calibrate_tsc();
      TSC_INITIAL = rdtsc();
      NS_CLOCK_FACTORS = {
            let mut sft = 32;
            let mut mul = 0;
            while sft > 0 {
                  mul = ((1000000 << sft) + (TSC_FREQ_KHZ >> 1)) / TSC_FREQ_KHZ;
                  if (mul >> 32) == 0 {
                        break;
                  }
                  sft -= 1;
            }
            (mul, sft)
      };
      log::info!("CPU Timestamp frequency: {} KHz", TSC_FREQ_KHZ);
}
