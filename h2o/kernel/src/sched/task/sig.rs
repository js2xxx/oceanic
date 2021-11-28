use alloc::sync::Arc;
use core::ptr;

use crate::sched::wait::WaitObject;

#[derive(Debug, Clone)]
pub enum Signal {
    Kill,
    Suspend(Arc<WaitObject>),
}

impl PartialEq for Signal {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Suspend(l0), Self::Suspend(r0)) => ptr::eq(&l0, &r0),
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

impl PartialOrd for Signal {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        match (self, other) {
            (Signal::Kill, Signal::Kill) => Some(core::cmp::Ordering::Equal),
            (Signal::Suspend(_), Signal::Kill) => Some(core::cmp::Ordering::Greater),
            (Signal::Kill, Signal::Suspend(_)) => Some(core::cmp::Ordering::Less),
            (Signal::Suspend(_), Signal::Suspend(_)) => None,
        }
    }
}
