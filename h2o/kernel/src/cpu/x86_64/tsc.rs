use archop::msr::rdtsc;
use raw_cpuid::CpuId;

use crate::cpu::time::{
    chip::{factor_from_freq, ClockChip},
    Instant,
};

pub struct TscClock {
    pub initial: u64,
    pub mul: u128,
    pub sft: u128,
}

impl TscClock {
    pub fn new() -> Self {
        if CpuId::new()
            .get_advanced_power_mgmt_info()
            .map_or(true, |info| !info.has_invariant_tsc())
        {
            log::warn!("The TSC is not invariant. Ticks will be unreliable.");
        }

        let khz = crate::cpu::time::chip::calibrate(|| {}, rdtsc, rdtsc, || {});

        let initial = rdtsc();
        let (mul, sft) = factor_from_freq(khz);
        log::info!("CPU Timestamp frequency: {} KHz", khz);
        TscClock { initial, mul, sft }
    }
}

impl Default for TscClock {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockChip for TscClock {
    fn get(&self) -> Instant {
        let val = rdtsc() - self.initial;
        let ns = (val as u128 * self.mul) >> self.sft;
        unsafe { Instant::from_raw(ns) }
    }
}
