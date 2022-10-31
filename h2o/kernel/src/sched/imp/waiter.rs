use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use core::{
    fmt::Debug,
    sync::atomic::{AtomicUsize, Ordering::*},
    time::Duration,
};

use archop::PreemptStateGuard;
use crossbeam_queue::SegQueue;
use spin::Mutex;
use sv_call::{
    call::Syscall,
    ipc::{SIG_READ, SIG_WRITE},
    Feature, Result, ENOSPC,
};

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
    pub fn new(
        event: &Arc<dyn Event>,
        level_triggered: bool,
        wake_all: bool,
        signal: usize,
    ) -> Arc<Self> {
        let ret = Arc::new(Blocker {
            wake_all,
            wo: WaitObject::new(),
            event: Arc::downgrade(event) as _,
            waiter_data: WaiterData::new(
                if level_triggered {
                    TriggerMode::Level
                } else {
                    TriggerMode::Edge
                },
                signal,
            ),
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

    fn on_cancel(&self, _: *const (), signal: usize) {
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
struct Request {
    key: usize,
    event: Weak<dyn Event>,
    waiter_data: WaiterData,
    syscall: Option<Syscall>,
}

#[derive(Debug)]
struct Ready {
    canceled: bool,
    signal: usize,
    request: Request,
}

#[derive(Debug)]
pub struct Dispatcher {
    next_key: AtomicUsize,
    event: Arc<BasicEvent>,

    capacity: usize,
    pending: Mutex<Vec<Request>>,
    ready: SegQueue<Ready>,
}

impl Dispatcher {
    pub fn new(capacity: usize) -> Result<Arc<Self>> {
        Ok(Arc::try_new(Dispatcher {
            next_key: AtomicUsize::new(1),
            event: BasicEvent::new(0),

            capacity,
            pending: Mutex::new(Vec::new()),
            ready: SegQueue::new(),
        })?)
    }

    pub fn event(&self) -> Weak<dyn Event> {
        Arc::downgrade(&self.event) as _
    }

    pub fn push(
        self: &Arc<Self>,
        event: &Arc<dyn Event>,
        waiter_data: WaiterData,
        syscall: Option<Syscall>,
    ) -> Result<usize> {
        let key = self.next_key.fetch_add(1, AcqRel);
        let req = Request {
            key,
            event: Arc::downgrade(event),
            waiter_data,
            syscall,
        };
        PREEMPT.scope(|| {
            let mut pending = self.pending.lock();
            if pending.len() >= self.capacity - self.ready.len() {
                return Err(ENOSPC);
            }
            pending.push(req);
            Ok(())
        })?;

        event.wait(Arc::clone(self) as _);
        Ok(key)
    }

    pub fn pop(
        self: &Arc<Self>,
        key: &mut usize,
        signal_slot: &mut usize,
    ) -> Option<(bool, Option<Syscall>)> {
        let Ready {
            canceled,
            signal,
            request,
        } = self.ready.pop()?;
        let res = if !canceled { request.syscall } else { None };
        self.event.notify(0, SIG_WRITE);
        *key = request.key;
        *signal_slot = signal;
        Some((canceled, res))
    }
}

impl Waiter for Dispatcher {
    fn waiter_data(&self) -> WaiterData {
        unimplemented!()
    }

    fn on_cancel(&self, event: *const (), signal: usize) {
        let mut has_cancel = false;

        PREEMPT.scope(|| {
            let mut pending = self.pending.lock();
            let iter = pending.drain_filter(|req| {
                let (e, _) = req.event.as_ptr().to_raw_parts();
                e == event && req.waiter_data.can_signal(signal, false)
            });
            iter.for_each(|request| {
                self.ready.push(Ready {
                    canceled: true,
                    signal,
                    request,
                });
                has_cancel = true;
            });
        });

        if has_cancel {
            self.event.notify(0, SIG_READ);
        }
    }

    fn on_notify(&self, _: usize) {
        unimplemented!()
    }

    fn try_on_notify(&self, event: *const (), signal: usize, on_wait: bool) -> bool {
        if self.ready.len() >= self.capacity {
            return false;
        }
        let mut has_notify = false;

        let empty = PREEMPT.scope(|| {
            let mut pending = self.pending.lock();
            let iter = pending.drain_filter(|req| {
                let (e, _) = req.event.as_ptr().to_raw_parts();
                e == event && req.waiter_data.can_signal(signal, on_wait)
            });
            iter.for_each(|request| {
                self.ready.push(Ready {
                    canceled: false,
                    signal,
                    request,
                });
                has_notify = true;
            });
            pending.is_empty()
        });

        if has_notify {
            self.event.notify(0, SIG_READ);
        }
        empty
    }
}

unsafe impl DefaultFeature for Dispatcher {
    #[inline]
    fn default_features() -> Feature {
        Feature::SEND | Feature::SYNC | Feature::READ | Feature::WRITE | Feature::WAIT
    }
}
