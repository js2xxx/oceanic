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
pub use sv_call::ipc::{SIG_GENERIC, SIG_READ, SIG_WRITE, SIG_TIMER};

pub use self::{
    arsc::Arsc,
    channel::{Channel, Packet},
};
use super::PREEMPT;
use crate::cpu::arch::apic::TriggerMode;

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

mod syscall {
    use sv_call::*;

    use super::*;
    use crate::{
        cpu::time,
        sched::{Blocker, SCHED},
    };

    #[syscall]
    fn obj_wait(hdl: Handle, timeout_us: u64, wake_all: bool, signal: usize) -> Result<usize> {
        let pree = PREEMPT.lock();
        let cur = unsafe { (*SCHED.current()).as_ref().ok_or(ESRCH) }?;

        let obj = cur.space().handles().get_ref(hdl)?;
        if !obj.features().contains(Feature::WAIT) {
            return Err(EPERM);
        }
        let event = obj.event().upgrade().ok_or(EPIPE)?;

        let blocker = Blocker::new(&event, wake_all, signal);
        blocker.wait(Some(pree), time::from_us(timeout_us))?;

        let (detach_ret, signal) = blocker.detach();
        if !detach_ret {
            return Err(ETIME);
        }
        Ok(signal)
    }

    #[syscall]
    fn obj_await(hdl: Handle, wake_all: bool, signal: usize) -> Result<Handle> {
        SCHED.with_current(|cur| {
            let obj = cur.space().handles().get_ref(hdl)?;
            if !obj.features().contains(Feature::WAIT) {
                return Err(EPERM);
            }
            let event = obj.event().upgrade().ok_or(EPIPE)?;

            let blocker = Blocker::new(&event, wake_all, signal);
            cur.space().handles().insert_raw(blocker, None)
        })
    }

    #[syscall]
    fn obj_awend(waiter: Handle, timeout_us: u64) -> Result<usize> {
        let pree = PREEMPT.lock();
        let cur = unsafe { (*SCHED.current()).as_ref().ok_or(ESRCH) }?;

        let blocker = cur.space().handles().get::<Blocker>(waiter)?;
        blocker.wait(Some(pree), time::from_us(timeout_us))?;

        let (detach_ret, signal) = Arc::clone(&blocker).detach();
        SCHED.with_current(|cur| cur.space().handles().remove::<Blocker>(waiter))?;

        if !detach_ret {
            Err(ETIME)
        } else {
            Ok(signal)
        }
    }
}
