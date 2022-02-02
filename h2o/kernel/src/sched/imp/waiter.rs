use alloc::sync::{Arc, Weak};
use core::{fmt::Debug, time::Duration};

use spin::Mutex;

use super::PREEMPT;
use crate::{
    cpu::arch::apic::TriggerMode,
    sched::{wait::WaitObject, Event, Waiter, WaiterData},
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

    pub fn wait<T>(&self, guard: T, timeout: Duration) -> bool {
        if timeout.is_zero() || PREEMPT.scope(|| self.status.lock().1 != 0) {
            true
        } else {
            self.wo.wait(guard, timeout, "Blocker::wait")
        }
    }

    pub fn detach(self: Arc<Self>) -> (bool, usize) {
        let (ret, signal) = PREEMPT.scope(|| *self.status.lock());
        if let Some(event) = self.event.upgrade() {
            if !self.wake_all {
                event.notify(self.waiter_data().signal(), 0);
            }
            let (not_signaled, newer) = event.unwait(&(self as _));
            (!not_signaled && ret, newer)
        } else {
            (ret, signal)
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