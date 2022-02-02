mod arsc;
pub mod basic;
mod channel;

use alloc::{sync::Arc, vec::Vec};
use core::{
    fmt::Debug,
    hint, mem,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};

use spin::Mutex;

pub use self::{
    arsc::Arsc,
    channel::{Channel, Packet},
};
use super::PREEMPT;
use crate::cpu::arch::apic::TriggerMode;

pub const SIG_GENERIC: usize = 0b001;
pub const SIG_READ: usize = 0b010;
pub const SIG_WRITE: usize = 0b100;

#[derive(Debug, Default)]
pub struct EventData {
    waiters: Mutex<Vec<Arc<dyn Waiter>>>,
    signal: AtomicUsize,
}

impl EventData {
    pub fn new(init_signal: usize) -> Self {
        EventData {
            waiters: Mutex::new(Vec::new()),
            signal: AtomicUsize::new(init_signal),
        }
    }

    #[inline]
    pub fn waiters(&self) -> &Mutex<Vec<Arc<dyn Waiter>>> {
        &self.waiters
    }

    #[inline]
    pub fn signal(&self) -> &AtomicUsize {
        &self.signal
    }
}

pub trait Event: Debug + Send + Sync {
    fn event_data(&self) -> &EventData;

    #[inline]
    fn wait(&self, waiter: Arc<dyn Waiter>) {
        self.wait_impl(waiter);
    }

    fn wait_impl(&self, waiter: Arc<dyn Waiter>) {
        if waiter.waiter_data().trigger_mode == TriggerMode::Level {
            let signal = self.event_data().signal().load(SeqCst);
            if signal & waiter.waiter_data().signal != 0 {
                waiter.on_notify(signal);
                return;
            }
        }
        PREEMPT.scope(|| self.event_data().waiters.lock().push(waiter));
    }

    fn unwait(&self, waiter: &Arc<dyn Waiter>) -> (bool, usize) {
        let signal = self.event_data().signal().load(SeqCst);
        let ret = PREEMPT.scope(|| {
            let mut waiters = self.event_data().waiters.lock();
            let pos = waiters.iter().position(|w| {
                let (this, _) = Arc::as_ptr(w).to_raw_parts();
                let (other, _) = Arc::as_ptr(waiter).to_raw_parts();
                this == other
            });
            match pos {
                Some(pos) => {
                    waiters.swap_remove(pos);
                    true
                }
                None => false,
            }
        });
        (ret, signal)
    }

    fn cancel(&self) {
        let signal = self.event_data().signal.load(SeqCst);

        let waiters = PREEMPT.scope(|| mem::take(&mut *self.event_data().waiters.lock()));
        for waiter in waiters {
            waiter.on_cancel(signal);
        }
    }

    #[inline]
    fn notify(&self, clear: usize, set: usize) {
        self.notify_impl(clear, set);
    }

    fn notify_impl(&self, clear: usize, set: usize) {
        let signal = loop {
            let prev = self.event_data().signal.load(SeqCst);
            let new = (prev & !clear) | set;
            if prev == new {
                return;
            }
            match self
                .event_data()
                .signal
                .compare_exchange_weak(prev, new, SeqCst, SeqCst)
            {
                Ok(_) if prev & new == new => return,
                Ok(_) => break new,
                _ => hint::spin_loop(),
            }
        };
        PREEMPT.scope(|| {
            let mut waiters = self.event_data().waiters.lock();
            let waiters = waiters.drain_filter(|w| signal & w.waiter_data().signal != 0);
            for waiter in waiters {
                waiter.on_notify(signal);
            }
        });
    }
}

#[derive(Debug)]
pub struct WaiterData {
    trigger_mode: TriggerMode,
    signal: usize,
}

impl WaiterData {
    pub fn new(trigger_mode: TriggerMode, signal: usize) -> Self {
        WaiterData {
            trigger_mode,
            signal,
        }
    }

    pub fn trigger_mode(&self) -> TriggerMode {
        self.trigger_mode
    }

    pub fn signal(&self) -> usize {
        self.signal
    }
}

pub trait Waiter: Debug + Send + Sync {
    fn waiter_data(&self) -> &WaiterData;

    fn on_cancel(&self, signal: usize);

    fn on_notify(&self, signal: usize);
}
