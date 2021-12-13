use alloc::sync::Arc;
use core::{
    sync::atomic::{AtomicBool, Ordering::*},
    time::Duration,
};

use canary::Canary;

use super::Instant;
use crate::{cpu::CpuLocalLazy, sched::deque::Worker};

#[thread_local]
static TIMER_QUEUE: CpuLocalLazy<Worker<Arc<Timer>>> = CpuLocalLazy::new(|| Worker::new_fifo());

pub struct Callback {
    func: fn(Arc<Timer>, Instant, *mut u8),
    arg: *mut u8,
}

impl Callback {
    pub fn new(func: fn(Arc<Timer>, Instant, *mut u8), arg: *mut u8) -> Self {
        Callback { func, arg }
    }

    pub fn call(&self, timer: Arc<Timer>, cur_time: Instant) {
        (self.func)(timer, cur_time, self.arg)
    }
}

pub struct Timer {
    canary: Canary<Timer>,
    callback: Callback,
    deadline: Instant,
    cancel: AtomicBool,
}

impl Timer {
    pub fn cancel(&self) -> bool {
        !self.cancel.swap(true, AcqRel)
    }

    pub fn activate(duration: Duration, callback: Callback) -> Arc<Self> {
        let ret = Arc::new(Timer {
            canary: Canary::new(),
            callback,
            deadline: Instant::now() + duration,
            cancel: AtomicBool::new(false),
        });
        TIMER_QUEUE.push(Arc::clone(&ret));
        ret
    }
}

pub unsafe fn tick() {
    let mut cnt = TIMER_QUEUE.len();
    while cnt > 0 {
        match TIMER_QUEUE.pop() {
            Some(timer) => {
                let cur_time = Instant::now();
                timer.canary.assert();
                if !timer.cancel.load(Acquire) {
                    if cur_time >= timer.deadline {
                        timer.callback.call(Arc::clone(&timer), cur_time);
                    } else {
                        TIMER_QUEUE.push(timer);
                    }
                }
            }
            None => break,
        }
        cnt -= 1;
    }
}
