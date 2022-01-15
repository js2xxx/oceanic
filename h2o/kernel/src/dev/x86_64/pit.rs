use alloc::sync::Arc;
use core::{
    ptr,
    sync::atomic::{AtomicU64, Ordering::*},
};

use archop::io::{Io, Port};
use spin::Lazy;

use super::ioapic;
use crate::cpu::{
    intr::{self, Handler, Interrupt, IrqReturn, IsaIrq},
    time::{
        chip::{CalibrationClock, ClockChip},
        Instant,
    },
};

const PIT_RATE: u64 = 1193182;

static PIT_TICKS: AtomicU64 = AtomicU64::new(0);

pub static PIT_CLOCK: Lazy<PitClock> = Lazy::new(PitClock::new);

pub struct PitClock {
    intr: Arc<Interrupt>,
}

impl ClockChip for PitClock {
    fn get(&self) -> Instant {
        let ms = PIT_TICKS.load(SeqCst);
        unsafe { Instant::from_raw((ms * 1_000_000) as u128) }
    }
}

impl CalibrationClock for PitClock {
    unsafe fn prepare(&self, ms: u64) {
        let latch = PIT_RATE * ms / 1000;
        let mut speaker = Port::<u8>::new(0x61);
        // Set the Gate high and disable speaker
        speaker.write((speaker.read() & !0x02) | 0x01);

        let mut pit = Port::<u8>::new(0x40);
        // Channel 2, mode 0 (one-shot), binary count
        pit.write_offset(3, 0xb0);
        pit.write_offset(2, (latch & 0xff) as u8);
    }

    unsafe fn cycle(&self, ms: u64) {
        let latch = PIT_RATE * ms / 1000;
        let speaker = Port::<u8>::new(0x61);
        let mut pit = Port::<u8>::new(0x40);
        pit.write_offset(2, (latch >> 8) as u8);
        while (speaker.read() & 0x20) == 0 {}
    }

    unsafe fn cleanup(&self) {
        let mut pit = Port::<u8>::new(0x40);
        pit.write_offset(3, 0x38);
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
        unsafe { intr.chip().lock().unmask(Arc::clone(&intr)) };

        PitClock { intr }
    }

    pub fn intr(&self) -> &Interrupt {
        &self.intr
    }
}

impl Default for PitClock {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

fn pit_handler(_: *mut u8) -> IrqReturn {
    PIT_TICKS.fetch_add(1, SeqCst);
    IrqReturn::SUCCESS
}
