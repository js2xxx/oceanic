mod imp;

use alloc::sync::Arc;

use archop::Azy;

pub use self::imp::Interrupt;
pub use super::arch::intr as arch;
use crate::{
    dev::{ioapic, Resource},
    sched::PREEMPT,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum IsaIrq {
    Pit = 0,
    Ps2Keyboard = 1,
    Pic2 = 2,
    Serial2 = 3,
    Serial1 = 4,
    Printer1 = 7,
    Rtc = 8,
    Ps2Mouse = 12,
    Ide0 = 14,
    Ide1 = 15,
}

pub type IntrHandler = fn(*mut u8);

static GSI_RES: Azy<Arc<Resource<u32>>> = Azy::new(|| {
    PREEMPT.scope(|| {
        let range = ioapic::chip()
            .lock()
            .gsi_range()
            .expect("Failed to get GSI range");
        Resource::new_root(archop::rand::get(), range)
    })
});

#[inline]
pub fn gsi_resource() -> &'static Arc<Resource<u32>> {
    &GSI_RES
}
