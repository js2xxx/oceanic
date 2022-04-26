use core::sync::atomic::Ordering::Release;

use archop::Azy;

use super::Instant;
use crate::{cpu::arch::tsc::TSC_CLOCK, dev::hpet::HPET_CLOCK};

pub static CLOCK: Azy<&'static dyn ClockChip> = Azy::new(|| {
    let ret: &'static dyn ClockChip = &*TSC_CLOCK;
    crate::log::HAS_TIME.store(true, Release);
    ret
});

static CALIB_CLOCK: Azy<&'static dyn CalibrationClock> =
    Azy::new(|| HPET_CLOCK.as_ref().expect("No available clock"));

pub trait ClockChip: Send + Sync {
    fn get(&self) -> Instant;
}

pub trait CalibrationClock: ClockChip {
    unsafe fn prepare(&self, ms: u64);

    unsafe fn cycle(&self, ms: u64);

    unsafe fn cleanup(&self);
}

/// Calibrates a clock chip using a calibration clock.
///
/// # Returns
///
/// The target clock's frequency in kHz.
pub fn calibrate(
    prepare: impl Fn(),
    get_start: impl Fn() -> u64,
    get_end: impl Fn() -> u64,
    cleanup: impl Fn(),
) -> u64 {
    let tries = 3;
    let iter_ms = [10u64, 20];
    let mut best = [u64::MAX, u64::MAX];
    for (best, &duration) in best.iter_mut().zip(iter_ms.iter()) {
        for _ in 0..tries {
            unsafe {
                CALIB_CLOCK.prepare(duration);
                prepare();

                let start = get_start();
                CALIB_CLOCK.cycle(duration);
                *best = (*best).min(get_end() - start);

                CALIB_CLOCK.cleanup();
                cleanup();
            }
        }
    }
    (best[1] - best[0]) / (iter_ms[1] - iter_ms[0])
}

pub fn factor_from_freq(khz: u64) -> (u128, u128) {
    let mut sft = 32;
    let mut mul = 0;
    while sft > 0 {
        mul = ((1000000 << sft) + (khz >> 1)) / khz;
        if (mul >> 32) == 0 {
            break;
        }
        sft -= 1;
    }
    (mul as u128, sft as u128)
}
