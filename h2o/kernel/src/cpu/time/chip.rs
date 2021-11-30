use alloc::boxed::Box;

use spin::Lazy;

use super::Instant;
use crate::{
    cpu::arch::tsc::TscClock,
    dev::{hpet::HpetClock, pit::PitClock},
};

pub trait ClockChip: Send + Sync {
    fn get(&self) -> Instant;
}

pub static CLOCK_CHIP: Lazy<Box<dyn ClockChip>> = Lazy::new(|| match TscClock::new() {
    Some(tsc) => box tsc,
    None => match HpetClock::new() {
        Some(hpet) => box hpet,
        None => box PitClock::new(),
    },
});

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
