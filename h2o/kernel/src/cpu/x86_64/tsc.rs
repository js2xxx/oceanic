use archop::msr::rdtsc;
use raw_cpuid::CpuId;

use crate::cpu::time::{
    chip::{factor_from_freq, ClockChip},
    Instant,
};

pub struct TscClock {
    initial: u64,
    mul: u128,
    sft: u128,
}

impl ClockChip for TscClock {
    fn get(&self) -> Instant {
        let val = rdtsc() - self.initial;
        let ns = (val as u128 * self.mul) >> self.sft;
        unsafe { Instant::from_raw(ns) }
    }
}

impl TscClock {
    pub fn new() -> Option<TscClock> {
        let cpuid = CpuId::new();
        cpuid
            .get_advanced_power_mgmt_info()?
            .has_invariant_tsc()
            .then(|| {
                let khz = crate::dev::hpet::calibrate_tsc()
                    .unwrap_or(unsafe { crate::dev::pit::calibrate_tsc() });
                let initial = rdtsc();
                let (mul, sft) = factor_from_freq(khz);
                log::info!("CPU Timestamp frequency: {} KHz", khz);
                TscClock {
                    initial,
                    mul: mul as u128,
                    sft: sft as u128,
                }
            })
    }
}
