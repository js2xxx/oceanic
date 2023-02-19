mod imp;

use alloc::sync::Arc;
use core::ops::Range;

use archop::Azy;
use sv_call::Feature;

pub use self::imp::Interrupt;
pub use super::arch::intr as arch;
use crate::sched::{task::hdl::DefaultFeature, PREEMPT};

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

#[derive(Debug, Clone)]
pub struct Msi {
    pub target_address: u32,
    pub target_data: u32,

    pub vecs: Range<u8>,
    pub cpu: usize,
}

pub type IntrHandler = fn(*mut u8);

pub struct IntrRes;

unsafe impl DefaultFeature for IntrRes {
    fn default_features() -> Feature {
        Feature::SEND | Feature::SYNC
    }
}

static INTR_RES: Azy<Arc<IntrRes>> = Azy::new(|| PREEMPT.scope(|| Arc::new(IntrRes)));

#[inline]
pub fn intr_resource() -> &'static Arc<IntrRes> {
    &INTR_RES
}
