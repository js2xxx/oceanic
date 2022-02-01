use alloc::collections::LinkedList;
use core::{
    cell::UnsafeCell,
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering::*},
    time::Duration,
};

use super::Instant;
use crate::sched::{ipc::Arsc, task, PREEMPT};

#[thread_local]
static TIMER_QUEUE: TimerQueue = TimerQueue::new();

struct TimerQueue {
    inner: UnsafeCell<LinkedList<Arsc<Timer>>>,
}

impl TimerQueue {
    const fn new() -> Self {
        TimerQueue {
            inner: UnsafeCell::new(LinkedList::new()),
        }
    }

    fn push(&self, timer: Arsc<Timer>) {
        let ddl = timer.deadline;
        PREEMPT.scope(|| {
            let queue = unsafe { &mut *self.inner.get() };
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
        PREEMPT.scope(|| {
            let queue = unsafe { &mut *self.inner.get() };
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

pub type CallbackArg = NonNull<task::Blocked>;

type CallbackFn = fn(Arsc<Timer>, Instant, CallbackArg);

#[derive(Debug)]
pub struct Callback {
    func: CallbackFn,
    arg: CallbackArg,
    fired: AtomicBool,
}

impl Callback {
    pub fn new(func: CallbackFn, arg: CallbackArg) -> Self {
        Callback {
            fired: AtomicBool::new(false),
            func,
            arg,
        }
    }

    pub fn call(&self, timer: Arsc<Timer>, cur_time: Instant) {
        (self.func)(timer, cur_time, self.arg);
        self.fired.store(true, Release);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Oneshot,
    // Periodic,
}

#[derive(Debug)]
pub struct Timer {
    ty: Type,
    callback: Callback,
    duration: Duration,
    deadline: Instant,
    cancel: AtomicBool,
}

impl Timer {
    pub fn activate(
        ty: Type,
        duration: Duration,
        callback: Callback,
    ) -> sv_call::Result<Arsc<Self>> {
        let ret = Arsc::try_new(Timer {
            ty,
            callback,
            duration,
            deadline: Instant::now() + duration,
            cancel: AtomicBool::new(false),
        })?;
        if duration < Duration::MAX {
            TIMER_QUEUE.push(Arsc::clone(&ret));
        }
        Ok(ret)
    }

    #[inline]
    pub fn ty(&self) -> Type {
        self.ty
    }

    #[inline]
    pub fn duration(&self) -> Duration {
        self.duration
    }

    pub fn cancel(self: &Arsc<Self>) -> bool {
        let ret = self.cancel.swap(true, AcqRel);
        TIMER_QUEUE.pop(self);
        ret
    }

    pub fn is_canceled(&self) -> bool {
        self.cancel.load(Acquire)
    }

    pub fn is_fired(&self) -> bool {
        self.callback.fired.load(Acquire)
    }

    pub fn callback_arg(&self) -> CallbackArg {
        self.callback.arg
    }
}

pub unsafe fn tick() {
    let now = Instant::now();
    PREEMPT.scope(|| {
        let queue = unsafe { &mut *TIMER_QUEUE.inner.get() };
        let mut cur = queue.cursor_front_mut();
        loop {
            match cur.current() {
                Some(t) if t.is_canceled() => {
                    cur.remove_current();
                }
                Some(t) if t.deadline <= now => {
                    let timer = cur.remove_current().unwrap();
                    if !timer.cancel() {
                        timer.callback.call(Arsc::clone(&timer), now);
                    }
                }
                _ => break,
            }
        }
    })
}
