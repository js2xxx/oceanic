pub mod basic;
mod channel;

use alloc::sync::Arc;
use core::{
    fmt::Debug,
    hash::BuildHasherDefault,
    hint,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};

pub use arsc_rs::Arsc;
use collection_ex::{CHashMap, FnvHasher};
pub use sv_call::ipc::{SIG_GENERIC, SIG_READ, SIG_TIMER, SIG_WRITE};

pub use self::channel::{Channel, Packet};
use super::PREEMPT;
use crate::cpu::arch::apic::TriggerMode;

type BH = BuildHasherDefault<FnvHasher>;

#[derive(Debug, Default)]
pub struct EventData {
    waiters: CHashMap<usize, Arc<dyn Waiter>, BH>,
    signal: AtomicUsize,
}

impl EventData {
    pub fn new(init_signal: usize) -> Self {
        EventData {
            waiters: Default::default(),
            signal: AtomicUsize::new(init_signal),
        }
    }

    #[inline]
    pub fn waiters(&self) -> &CHashMap<usize, Arc<dyn Waiter>, BH> {
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
        let signal = self.event_data().signal().load(SeqCst);
        if waiter.try_on_notify(self as *const _ as _, signal, true) {
            return;
        }
        let (key, _) = Arc::as_ptr(&waiter).to_raw_parts();
        PREEMPT.scope(|| self.event_data().waiters.insert(key as _, waiter));
    }

    fn unwait(&self, waiter: &Arc<dyn Waiter>) -> (bool, usize) {
        let signal = self.event_data().signal().load(SeqCst);
        let ret = PREEMPT.scope(|| {
            let (other, _) = Arc::as_ptr(waiter).to_raw_parts();
            self.event_data()
                .waiters
                .remove(&(other as usize))
                .is_some()
        });
        (ret, signal)
    }

    fn cancel(&self) {
        let signal = self.event_data().signal.load(SeqCst);

        let waiters = PREEMPT.scope(|| self.event_data().waiters.take());
        for (_, waiter) in waiters {
            waiter.on_cancel(self as *const _ as _, signal);
        }
    }

    #[inline]
    fn notify(&self, clear: usize, set: usize) -> usize {
        self.notify_impl(clear, set)
    }

    fn notify_impl(&self, clear: usize, set: usize) -> usize {
        let mut prev = self.event_data().signal.load(SeqCst);
        let signal = loop {
            let new = (prev & !clear) | set;
            if prev == new {
                return prev;
            }
            match self
                .event_data()
                .signal
                .compare_exchange_weak(prev, new, SeqCst, SeqCst)
            {
                Ok(_) if prev & new == new => return new,
                Ok(_) => break new,
                Err(signal) => {
                    prev = signal;
                    hint::spin_loop()
                }
            }
        };
        PREEMPT.scope(|| {
            self.event_data()
                .waiters
                .retain(|_, waiter| !waiter.try_on_notify(self as *const _ as _, signal, false))
        });
        signal
    }
}

#[derive(Debug, Clone, Copy)]
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

    #[inline]
    pub fn can_signal(&self, signal: usize, on_wait: bool) -> bool {
        if on_wait && self.trigger_mode == TriggerMode::Edge {
            false
        } else {
            self.signal & !signal == 0
        }
    }
}

pub trait Waiter: Debug + Send + Sync {
    fn waiter_data(&self) -> WaiterData;

    fn on_cancel(&self, event: *const (), signal: usize);

    fn on_notify(&self, signal: usize);

    #[inline]
    fn try_on_notify(&self, _: *const (), signal: usize, on_wait: bool) -> bool {
        let ret = self.waiter_data().can_signal(signal, on_wait);
        if ret {
            self.on_notify(signal);
        }
        ret
    }
}

mod syscall {
    use sv_call::{call::Syscall, *};

    use super::*;
    use crate::{
        cpu::{arch::apic::TriggerMode, time},
        sched::{BasicEvent, Blocker, Dispatcher, WaiterData, SCHED},
        syscall::{In, Out, UserPtr},
    };

    #[syscall]
    fn event_new(init_signal: usize) -> Result<Handle> {
        let obj = BasicEvent::new(init_signal);
        let event = Arc::downgrade(&obj) as _;
        SCHED.with_current(|cur| cur.space().handles().insert_raw(obj, Some(event)))
    }

    #[syscall]
    fn event_notify(hdl: Handle, clear: usize, set: usize) -> Result<usize> {
        hdl.check_null()?;
        let event = SCHED.with_current(|cur| {
            cur.space()
                .handles()
                .get::<BasicEvent>(hdl)
                .map(|event| Arc::clone(&event))
        })?;
        Ok(event.notify(clear, set))
    }

    #[syscall]
    fn event_cancel(hdl: Handle) -> Result {
        hdl.check_null()?;
        let event = SCHED.with_current(|cur| {
            cur.space()
                .handles()
                .get::<BasicEvent>(hdl)
                .map(|event| Arc::clone(&event))
        })?;
        event.cancel();
        Ok(())
    }

    #[syscall]
    fn obj_wait(
        hdl: Handle,
        timeout_us: u64,
        level_triggered: bool,
        wake_all: bool,
        signal: usize,
    ) -> Result<usize> {
        let pree = PREEMPT.lock();
        let cur = unsafe { (*SCHED.current()).as_ref().ok_or(ESRCH) }?;

        let obj = cur.space().handles().get_ref(hdl)?;
        if !obj.features().contains(Feature::WAIT) {
            return Err(EPERM);
        }
        let event = obj.event().upgrade().ok_or(EPIPE)?;
        drop(obj);

        let blocker = Blocker::new(&event, level_triggered, wake_all, signal);
        blocker.wait(Some(pree), time::from_us(timeout_us))?;

        let (detach_ret, signal) = blocker.detach();
        if !detach_ret {
            return Err(ETIME);
        }
        Ok(signal)
    }

    #[syscall]
    fn disp_new(capacity: usize) -> Result<Handle> {
        let disp = Dispatcher::new(capacity)?;
        let event = disp.event();
        SCHED.with_current(|cur| cur.space().handles().insert_raw(disp, Some(event)))
    }

    #[syscall]
    fn disp_push(
        disp: Handle,
        hdl: Handle,
        level_triggered: bool,
        signal: usize,
        syscall: UserPtr<In, Syscall>,
    ) -> Result<usize> {
        hdl.check_null()?;
        disp.check_null()?;
        let syscall = (!syscall.as_ptr().is_null())
            .then(|| {
                let syscall = unsafe { syscall.read() }?;
                if matches!(syscall.num, SV_DISP_NEW | SV_DISP_PUSH | SV_DISP_POP) {
                    return Err(EPERM);
                }
                Ok(syscall)
            })
            .transpose()?;

        SCHED.with_current(|cur| {
            let obj = cur.space().handles().get_ref(hdl)?;
            let disp = cur.space().handles().get::<Dispatcher>(disp)?;
            if !obj.features().contains(Feature::WAIT) {
                return Err(EPERM);
            }
            if !disp.features().contains(Feature::WRITE) {
                return Err(EPERM);
            }
            let event = obj.event().upgrade().ok_or(EPIPE)?;
            drop(obj);

            let waiter_data = WaiterData::new(
                if level_triggered {
                    TriggerMode::Level
                } else {
                    TriggerMode::Edge
                },
                signal,
            );
            disp.push(&event, waiter_data, syscall)
        })
    }

    #[syscall]
    fn disp_pop(
        disp: Handle,
        signal_slot: UserPtr<Out, usize>,
        result: UserPtr<Out, usize>,
    ) -> Result<usize> {
        disp.check_null()?;

        let mut key = 0;
        let mut signal = 0;
        let (canceled, r) = SCHED.with_current(|cur| {
            let disp = cur.space().handles().get::<Dispatcher>(disp)?;
            if !disp.features().contains(Feature::READ) {
                return Err(EPERM);
            }
            disp.pop(&mut key, &mut signal).ok_or(ENOENT)
        })?;

        if !signal_slot.as_ptr().is_null() {
            signal_slot.write(if canceled { 0 } else { signal })?;
        }

        let r = r.map_or(0, crate::syscall::handle);
        if !result.as_ptr().is_null() {
            result.write(r)?;
        }
        Ok(key)
    }
}
