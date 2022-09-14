use alloc::{collections::LinkedList, sync::Weak};
use core::{
    cell::RefCell,
    sync::atomic::{AtomicBool, Ordering::*},
    time::Duration,
};

use spin::RwLock;
use sv_call::ipc::SIG_TIMER;

use super::Instant;
use crate::sched::{ipc::Arsc, task, Event, PREEMPT, SCHED};

#[thread_local]
static TIMER_QUEUE: TimerQueue = TimerQueue::new();

struct TimerQueue {
    inner: RefCell<LinkedList<Arsc<Timer>>>,
}

impl TimerQueue {
    const fn new() -> Self {
        TimerQueue {
            inner: RefCell::new(LinkedList::new()),
        }
    }

    #[inline]
    #[track_caller]
    fn with_inner<F, R>(&self, func: F) -> R
    where
        F: FnOnce(&mut LinkedList<Arsc<Timer>>) -> R,
    {
        PREEMPT.scope(|| func(&mut self.inner.borrow_mut()))
    }

    #[inline]
    fn try_with_inner<F, R>(&self, func: F) -> Option<R>
    where
        F: FnOnce(&mut LinkedList<Arsc<Timer>>) -> R,
    {
        PREEMPT.scope(|| self.inner.try_borrow_mut().ok().map(|mut r| func(&mut r)))
    }

    fn push(&self, timer: Arsc<Timer>) {
        let ddl = timer.deadline;
        self.with_inner(|queue| {
            let mut cur = queue.cursor_front_mut();
            loop {
                match cur.current() {
                    Some(t) if t.deadline >= ddl => {
                        cur.insert_before(timer);
                        break;
                    }
                    None => {
                        cur.insert_before(timer);
                        break;
                    }
                    Some(_) => cur.move_next(),
                }
            }
        })
    }

    fn pop(&self, timer: &Arsc<Timer>) -> bool {
        self.with_inner(|queue| {
            let mut cur = queue.cursor_front_mut();
            loop {
                match cur.current() {
                    Some(t) if Arsc::ptr_eq(t, timer) => {
                        cur.remove_current();
                        break true;
                    }
                    Some(_) => cur.move_next(),
                    None => break false,
                }
            }
        })
    }
}

#[derive(Debug)]
pub enum Callback {
    Task(task::Blocked),
    Event(Weak<dyn Event>),
}

impl From<task::Blocked> for Callback {
    fn from(task: task::Blocked) -> Self {
        Self::Task(task)
    }
}

impl From<Weak<dyn Event>> for Callback {
    fn from(event: Weak<dyn Event>) -> Self {
        Self::Event(event)
    }
}

impl Callback {
    fn call(self, timer: &Timer) {
        timer.fired.store(true, Release);
        match self {
            Callback::Task(task) => SCHED.unblock(task, true),
            Callback::Event(event) => {
                if let Some(event) = event.upgrade() {
                    event.notify(0, SIG_TIMER)
                }
            }
        }
    }

    fn cancel(self, preempt: bool) {
        match self {
            Callback::Task(task) => SCHED.unblock(task, preempt),
            Callback::Event(event) => {
                if let Some(event) = event.upgrade() {
                    event.cancel()
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct Timer {
    callback: RwLock<Option<Callback>>,
    deadline: Instant,
    fired: AtomicBool,
}

impl Timer {
    pub fn activate<C: Into<Callback>>(
        duration: Duration,
        callback: C,
    ) -> sv_call::Result<Arsc<Self>> {
        let ret = Arsc::try_new(Timer {
            callback: RwLock::new(Some(callback.into())),
            deadline: Instant::now() + duration,
            fired: AtomicBool::new(false),
        })?;
        if duration < Duration::MAX {
            TIMER_QUEUE.push(Arsc::clone(&ret));
        }
        Ok(ret)
    }

    pub fn cancel(self: &Arsc<Self>, preempt: bool) -> bool {
        TIMER_QUEUE.pop(self);
        match PREEMPT.scope(|| self.callback.write().take()) {
            Some(callback) => {
                callback.cancel(preempt);
                true
            }
            None => false,
        }
    }

    pub fn fire(&self) {
        if let Some(callback) = PREEMPT.scope(|| self.callback.write().take()) {
            callback.call(self);
        }
    }

    pub fn is_fired(&self) -> bool {
        self.fired.load(Acquire)
    }
}

pub unsafe fn tick() {
    loop {
        let now = Instant::now();
        let timer = TIMER_QUEUE.try_with_inner(|queue| {
            let mut cur = queue.cursor_front_mut();
            loop {
                match cur.current() {
                    Some(timer) if timer.callback.try_read().map_or(false, |r| r.is_none()) => {
                        cur.remove_current();
                    }
                    Some(timer) if timer.deadline <= now => {
                        break cur.remove_current();
                    }
                    _ => break None,
                }
            }
        });
        match timer {
            Some(Some(timer)) => timer.fire(),
            _ => break,
        }
    }
}

mod syscall {
    use alloc::sync::{Arc, Weak};

    use spin::Mutex;
    use sv_call::*;

    use super::Timer;
    use crate::{
        cpu::time,
        sched::{task::hdl::DefaultFeature, Arsc, Event, EventData, SCHED},
    };

    #[derive(Debug, Default)]
    struct TimerEvent {
        event_data: EventData,
        timer: Mutex<Option<Arsc<Timer>>>,
    }

    unsafe impl Send for TimerEvent {}
    unsafe impl Sync for TimerEvent {}

    impl Event for TimerEvent {
        fn event_data(&self) -> &EventData {
            &self.event_data
        }
    }

    impl Drop for TimerEvent {
        fn drop(&mut self) {
            match self.timer.get_mut().take() {
                Some(timer) => {
                    timer.cancel(false);
                }
                None => self.cancel(),
            }
        }
    }

    unsafe impl DefaultFeature for TimerEvent {
        fn default_features() -> sv_call::Feature {
            Feature::SEND | Feature::SYNC | Feature::WAIT | Feature::WRITE
        }
    }

    #[syscall]
    fn timer_new() -> Result<Handle> {
        let event = Arc::try_new(TimerEvent::default())?;
        let e = Arc::downgrade(&event);
        SCHED.with_current(|cur| cur.space().handles().insert_raw(event, Some(e)))
    }

    #[syscall]
    fn timer_set(handle: Handle, duration_us: u64) -> Result {
        SCHED.with_current(|cur| {
            let event = cur.space().handles().get::<TimerEvent>(handle)?;

            if !event.features().contains(Feature::WRITE) {
                return Err(EPERM);
            }

            let mut timer = event.timer.lock();
            if let Some(timer) = timer.take() {
                timer.cancel(false);
            }
            if duration_us > 0 {
                *timer = Some(Timer::activate(
                    time::from_us(duration_us),
                    Weak::clone(event.event()),
                )?);
            }
            Ok(())
        })
    }
}
