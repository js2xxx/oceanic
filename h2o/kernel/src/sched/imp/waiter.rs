use alloc::sync::{Arc, Weak};
use core::{fmt::Debug, time::Duration};

use spin::Mutex;
use sv_call::Feature;

use super::PREEMPT;
use crate::{
    cpu::arch::apic::TriggerMode,
    sched::{task::hdl::DefaultFeature, wait::WaitObject, Event, Waiter, WaiterData},
};

#[derive(Debug)]
pub struct Blocker {
    wake_all: bool,
    wo: WaitObject,
    event: Weak<dyn Event>,
    waiter_data: WaiterData,
    status: Mutex<(bool, usize)>,
}

impl Blocker {
    pub fn new(event: &Arc<dyn Event>, wake_all: bool, signal: usize) -> Arc<Self> {
        let ret = Arc::new(Blocker {
            wake_all,
            wo: WaitObject::new(),
            event: Arc::downgrade(event) as _,
            waiter_data: WaiterData::new(TriggerMode::Level, signal),
            status: Mutex::new((true, 0)),
        });
        event.wait(Arc::clone(&ret) as _);
        ret
    }

    pub fn wait<T>(&self, guard: T, timeout: Duration) -> sv_call::Result {
        let pree = PREEMPT.lock();
        let status = self.status.lock();
        if timeout.is_zero() || status.1 != 0 {
            Ok(())
        } else {
            self.wo.wait((guard, status, pree), timeout, "Blocker::wait")
        }
    }

    pub fn detach(self: Arc<Self>) -> (bool, usize) {
        let (has_signal, signal) = PREEMPT.scope(|| *self.status.lock());
        if let Some(event) = self.event.upgrade() {
            let (wait_for, wake_all) = (self.waiter_data().signal(), self.wake_all);
            let (not_signaled, newer) = event.unwait(&(self as _));
            let has_signal = !not_signaled && has_signal;
            if !wake_all && has_signal {
                event.notify(wait_for, 0);
            }
            (has_signal, newer)
        } else {
            (has_signal, signal)
        }
    }
}

impl Waiter for Blocker {
    #[inline]
    fn waiter_data(&self) -> &WaiterData {
        &self.waiter_data
    }

    fn on_cancel(&self, signal: usize) {
        PREEMPT.scope(|| *self.status.lock() = (false, signal));
        let num = if self.wake_all { usize::MAX } else { 1 };
        self.wo.notify(num, false);
    }

    fn on_notify(&self, signal: usize) {
        PREEMPT.scope(|| *self.status.lock() = (true, signal));
        let num = if self.wake_all { usize::MAX } else { 1 };
        self.wo.notify(num, false);
    }
}

unsafe impl DefaultFeature for Arc<Blocker> {
    fn default_features() -> sv_call::Feature {
        Feature::SEND | Feature::WAIT
    }
}
