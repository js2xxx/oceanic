use alloc::sync::Arc;

use crate::{cpu::time::Instant, sched::wait::WaitObject};

#[derive(Debug, Clone)]
pub enum Signal {
    Kill,
    Suspend(Arc<WaitObject>, Instant),
}

impl PartialEq for Signal {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Suspend(l0, l1), Self::Suspend(r0, r1)) => Arc::ptr_eq(&l0, &r0) && l1 == r1,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

impl PartialOrd for Signal {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        match (self, other) {
            (Signal::Kill, Signal::Kill) => Some(core::cmp::Ordering::Equal),
            (Signal::Suspend(..), Signal::Kill) => Some(core::cmp::Ordering::Greater),
            (Signal::Kill, Signal::Suspend(..)) => Some(core::cmp::Ordering::Less),
            (Signal::Suspend(_, t0), Signal::Suspend(_, t1)) => t0.partial_cmp(t1),
        }
    }
}
