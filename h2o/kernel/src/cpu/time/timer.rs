use alloc::sync::Arc;
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering::*},
    time::Duration,
};

use canary::Canary;

use super::Instant;
use crate::{cpu::CpuLocalLazy, sched::deque::Worker};

#[thread_local]
static TIMER_QUEUE: CpuLocalLazy<Worker<Arc<Timer>>> = CpuLocalLazy::new(|| Worker::new_fifo());

#[derive(Debug)]
pub struct Callback {
    func: fn(Arc<Timer>, Instant, *mut u8),
    arg: *mut u8,
    fired: AtomicBool,
}

impl Callback {
    pub fn new(func: fn(Arc<Timer>, Instant, *mut u8), arg: *mut u8) -> Self {
        Callback {
            fired: AtomicBool::new(false),
            func,
            arg,
        }
    }

    pub fn call(&self, timer: Arc<Timer>, cur_time: Instant) {
        (self.func)(timer, cur_time, self.arg);
        self.fired.store(true, Release);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Oneshot,
    Periodic,
}

#[derive(Debug)]
pub struct Timer {
    canary: Canary<Timer>,
    ty: Type,
    callback: Callback,
    duration: Duration,
    deadline: UnsafeCell<Instant>,
    cancel: AtomicBool,
}

impl Timer {
    pub fn activate(ty: Type, duration: Duration, callback: Callback) -> Arc<Self> {
        let ret = Arc::new(Timer {
            canary: Canary::new(),
            ty,
            callback,
            duration,
            deadline: UnsafeCell::new(Instant::now() + duration),
            cancel: AtomicBool::new(false),
        });
        if duration < Duration::MAX {
            TIMER_QUEUE.push(Arc::clone(&ret));
        }
        ret
    }

    pub fn ty(&self) -> Type {
        self.ty
    }

    pub fn cancel(&self) -> bool {
        self.cancel.swap(true, AcqRel)
    }

    pub fn is_canceled(&self) -> bool {
        self.cancel.load(Acquire)
    }

    pub fn is_fired(&self) -> bool {
        self.callback.fired.load(Acquire)
    }

    pub fn callback_arg(&self) -> *mut u8 {
        self.callback.arg
    }

    unsafe fn handle(timer: Arc<Self>, cur_time: Instant) {
        timer.canary.assert();
        if !timer.is_canceled() {
            if cur_time >= *timer.deadline.get() {
                match timer.ty {
                    Type::Oneshot => {
                        if !timer.cancel() {
                            timer.callback.call(Arc::clone(&timer), cur_time);
                        }
                    }
                    Type::Periodic => {
                        *timer.deadline.get() += timer.duration;
                        timer.callback.call(Arc::clone(&timer), cur_time);
                        TIMER_QUEUE.push(timer);
                    }
                }
            } else {
                TIMER_QUEUE.push(timer);
            }
        }
    }
}

pub unsafe fn tick() {
    let mut cnt = TIMER_QUEUE.len();
    while cnt > 0 {
        match TIMER_QUEUE.pop() {
            Some(timer) => Timer::handle(timer, Instant::now()),
            None => break,
        }
        cnt -= 1;
    }
}
