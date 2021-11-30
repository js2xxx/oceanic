use alloc::sync::Arc;
use core::{
    ptr,
    sync::atomic::{AtomicU64, Ordering::*},
};

use archop::{
    io::{Io, Port},
    msr::rdtsc,
};
use static_assertions::const_assert;

use super::ioapic;
use crate::cpu::{
    intr::{self, Handler, Interrupt, IrqReturn, IsaIrq},
    time::{chip::ClockChip, Instant},
};

const PIT_RATE: u64 = 1193182;

static PIT_TICKS: AtomicU64 = AtomicU64::new(0);

/// Calibrate the CPU's frequency (KHz) by activating the PIT timer.
pub unsafe fn calibrate_tsc() -> u64 {
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

pub struct PitClock {
    intr: Arc<Interrupt>,
}

impl ClockChip for PitClock {
    fn get(&self) -> Instant {
        let ms = PIT_TICKS.load(SeqCst);
        unsafe { Instant::from_raw((ms * 1_000_000) as u128) }
    }
}

impl PitClock {
    pub fn new() -> PitClock {
        unsafe {
            let mut speaker = Port::<u8>::new(0x61);
            // Set the Gate high and disable speaker
            speaker.write((speaker.read() & !0x02) | 0x01);

            let divisor = PIT_RATE / 1000;
            let mut pit = Port::<u8>::new(0x40);
            pit.write_offset(3, 0x34);
            pit.write_offset(2, (divisor & 0xff) as u8);
            pit.write_offset(2, (divisor >> 8) as u8);
        }

        let irq = IsaIrq::Pit;
        let gsi = ioapic::gsi_from_isa(irq);
        let intr = intr::ALLOC
            .lock()
            .alloc_setup(gsi, irq as u8, ioapic::chip(), crate::cpu::all_mask())
            .expect("Failed to allocate interrupt");
        intr.handlers()
            .lock()
            .push(Handler::new(pit_handler, ptr::null_mut()));
        unsafe { intr.chip().lock().unmask(intr.clone()) };

        PitClock { intr }
    }

    pub fn intr(&self) -> &Interrupt {
        &self.intr
    }
}

fn pit_handler(_: *mut u8) -> IrqReturn {
    PIT_TICKS.fetch_add(1, SeqCst);
    IrqReturn::SUCCESS
}
