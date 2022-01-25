use alloc::sync::Arc;

use spin::Mutex;

#[derive(Debug, Clone)]
pub enum Signal {
    Kill,
    Suspend(Arc<Mutex<Option<super::Blocked>>>),
}

impl PartialEq for Signal {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Suspend(a), Self::Suspend(b)) => Arc::ptr_eq(a, b),
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
            _ => None,
        }
    }
}
