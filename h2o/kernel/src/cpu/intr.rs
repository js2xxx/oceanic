mod imp;

use alloc::sync::Arc;

use archop::Azy;
use sv_call::Feature;

pub use self::imp::Interrupt;
pub use super::arch::intr as arch;
use crate::sched::{task::hdl::DefaultFeature, PREEMPT};

pub type IntrHandler = fn(*const Interrupt);

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
