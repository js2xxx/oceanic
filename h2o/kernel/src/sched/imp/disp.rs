use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use core::{
    alloc::AllocError,
    sync::atomic::{AtomicUsize, Ordering::*},
};

use crossbeam_queue::ArrayQueue;
use spin::Mutex;
use sv_call::{call::Syscall, Feature, Result, ENOSPC};

use super::PREEMPT;
use crate::{
    sched::{task::hdl::DefaultFeature, BasicEvent, Event, Waiter, WaiterData},
    syscall,
};

#[derive(Debug)]
struct Request {
    key: usize,
    event: Weak<dyn Event>,
    waiter_data: WaiterData,
    syscall: Syscall,
}

#[derive(Debug)]
pub struct Dispatcher {
    next_key: AtomicUsize,
    event: Arc<BasicEvent>,

    capacity: usize,
    pending: Mutex<Vec<Request>>,
    ready: ArrayQueue<(bool, Request)>,
}

impl Dispatcher {
    pub fn new(capacity: usize) -> Result<Arc<Self>> {
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(capacity)
            .map_err(|_| AllocError)?;
        Ok(Arc::try_new(Dispatcher {
            next_key: AtomicUsize::new(1),
            event: BasicEvent::new(0),

            capacity,
            pending: Mutex::new(pending),
            ready: ArrayQueue::new(capacity),
        })?)
    }

    pub fn event(&self) -> Weak<dyn Event> {
        Arc::downgrade(&self.event) as _
    }

    pub fn push(
        self: &Arc<Self>,
        event: &Arc<dyn Event>,
        waiter_data: WaiterData,
        syscall: Syscall,
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
            if pending.len() >= self.capacity - self.ready.capacity() {
                return Err(ENOSPC);
            }
            pending.push(req);
            Ok(())
        })?;

        event.wait(Arc::clone(self) as _);
        Ok(key)
    }

    pub fn pop(self: &Arc<Self>) -> Option<(bool, usize, usize)> {
        let (canceled, req) = self.ready.pop()?;
        if let Some(event) = req.event.upgrade() {
            event.unwait(&(Arc::clone(self) as _));
        }
        let res = if !canceled {
            syscall::handle(req.syscall)
        } else {
            0
        };
        Some((canceled, req.key, res))
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
            iter.for_each(|req| {
                self.ready.push((false, req)).unwrap();
                has_cancel = true;
            });
        });

        if has_cancel {}
    }

    fn on_notify(&self, _: usize) {
        unimplemented!()
    }

    fn try_on_notify(&self, event: *const (), signal: usize, on_wait: bool) -> bool {
        if self.ready.is_full() {
            return false;
        }
        let mut has_notify = false;

        let empty = PREEMPT.scope(|| {
            let mut pending = self.pending.lock();
            let iter = pending.drain_filter(|req| {
                let (e, _) = req.event.as_ptr().to_raw_parts();
                e == event && req.waiter_data.can_signal(signal, on_wait)
            });
            iter.for_each(|req| {
                self.ready.push((false, req)).unwrap();
                has_notify = true;
            });
            pending.is_empty()
        });

        if has_notify {}
        empty
    }
}

unsafe impl DefaultFeature for Dispatcher {
    #[inline]
    fn default_features() -> Feature {
        Feature::SEND | Feature::SYNC | Feature::READ | Feature::WRITE | Feature::WAIT
    }
}
