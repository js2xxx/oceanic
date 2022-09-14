use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use core::{
    fmt::Debug,
    sync::atomic::{AtomicUsize, Ordering::AcqRel},
    time::Duration,
};

use archop::PreemptStateGuard;
use crossbeam_queue::SegQueue;
use spin::Mutex;
use sv_call::{ipc::SIG_READ, Feature};

use super::PREEMPT;
use crate::{
    cpu::arch::apic::TriggerMode,
    sched::{task::hdl::DefaultFeature, wait::WaitObject, BasicEvent, Event, Waiter, WaiterData},
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

    pub fn wait(&self, pree: Option<PreemptStateGuard>, timeout: Duration) -> sv_call::Result {
        let pree = match pree {
            Some(pree) => pree,
            None => PREEMPT.lock(),
        };
        let status = self.status.lock();
        if timeout.is_zero() || status.1 != 0 {
            Ok(())
        } else if self.event.strong_count() == 0 {
            Err(sv_call::EPIPE)
        } else {
            self.wo.wait((status, pree), timeout, "Blocker::wait")
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
    fn waiter_data(&self) -> WaiterData {
        self.waiter_data
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

unsafe impl DefaultFeature for Blocker {
    #[inline]
    fn default_features() -> sv_call::Feature {
        Feature::SEND
    }
}

#[derive(Debug)]
pub struct Dispatcher {
    next_key: AtomicUsize,
    event: Arc<BasicEvent>,

    #[allow(clippy::type_complexity)]
    waiters: Mutex<Vec<(usize, Weak<dyn Event>, WaiterData)>>,
    triggered: SegQueue<(usize, Weak<dyn Event>, bool)>,
}

impl Dispatcher {
    pub fn new() -> Self {
        Dispatcher {
            next_key: AtomicUsize::new(1),
            event: BasicEvent::new(0),
            waiters: Mutex::new(Vec::new()),
            triggered: SegQueue::new(),
        }
    }

    #[inline]
    pub fn event(&self) -> Weak<dyn Event> {
        Arc::downgrade(&self.event) as _
    }

    pub fn push(self: &Arc<Self>, event: &Arc<dyn Event>, data: WaiterData) -> usize {
        let key = self.next_key.fetch_add(1, AcqRel);
        PREEMPT.scope(|| {
            let mut waiters = self.waiters.lock();
            waiters.push((key, Arc::downgrade(event), data));
        });
        event.wait(Arc::clone(self) as _);
        key
    }

    #[inline]
    pub fn pop(self: &Arc<Self>) -> Option<(usize, bool)> {
        let (key, event, canceled) = self.triggered.pop()?;
        if let Some(event) = event.upgrade() {
            event.unwait(&(Arc::clone(self) as _));
        }
        Some((key, canceled))
    }
}

impl Waiter for Dispatcher {
    fn waiter_data(&self) -> WaiterData {
        let (trig, signal) = PREEMPT.scope(|| {
            let waiters = self.waiters.lock();
            let iter = waiters.iter();
            iter.fold((TriggerMode::Edge, 0), |(trig, signal), (_, _, data)| {
                (trig | data.trigger_mode(), signal | data.signal())
            })
        });
        WaiterData::new(trig, signal)
    }

    fn on_cancel(&self, signal: usize) {
        let mut has_cancel = false;
        PREEMPT.scope(|| {
            let mut waiters = self.waiters.lock();
            let iter = waiters.drain_filter(|(_, _, data)| data.signal() & !signal == 0);
            iter.for_each(|(key, event, _)| {
                self.triggered.push((key, event.clone(), false));
                has_cancel = true;
            })
        });
        if has_cancel {
            self.event.notify(0, SIG_READ)
        }
    }

    fn on_notify(&self, signal: usize) {
        let mut has_notify = false;
        PREEMPT.scope(|| {
            let mut waiters = self.waiters.lock();
            let iter = waiters.drain_filter(|(_, _, data)| data.signal() & !signal == 0);
            iter.for_each(|(key, event, _)| {
                self.triggered.push((key, event.clone(), true));
                has_notify = true
            })
        });
        if has_notify {
            self.event.notify(0, SIG_READ)
        }
    }
}

unsafe impl DefaultFeature for Dispatcher {
    #[inline]
    fn default_features() -> Feature {
        Feature::SEND | Feature::SYNC | Feature::READ | Feature::WRITE | Feature::WAIT
    }
}
