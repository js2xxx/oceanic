mod cell;
mod futex;
mod queue;

use alloc::{boxed::Box, sync::Arc};
use core::{ptr::NonNull, time::Duration};

use crossbeam_queue::SegQueue;
use sv_call::ipc::EVENT_SIG_ASYNC;

pub use self::{cell::WaitCell, futex::*, queue::WaitQueue};
use super::{
    ipc::{Arsc, Event},
    *,
};
use crate::cpu::time::{CallbackArg, Instant, Timer, TimerCallback, TimerType};

#[derive(Debug)]
pub struct WaitObject {
    pub(super) wait_queue: SegQueue<Arsc<Timer>>,
}

unsafe impl Send for WaitObject {}
unsafe impl Sync for WaitObject {}

impl WaitObject {
    #[inline]
    pub fn new() -> Self {
        WaitObject {
            wait_queue: SegQueue::new(),
        }
    }

    #[inline]
    pub fn wait<T>(&self, guard: T, timeout: Duration, block_desc: &'static str) -> bool {
        let timer = SCHED.block_current(guard, Some(self), timeout, block_desc);
        timer.map_or(false, |timer| !timer.is_fired())
    }

    #[inline]
    pub fn wait_async(&self, event: Arc<Event>, timeout: Duration) -> sv_call::Result {
        let timer = Timer::activate(
            TimerType::Oneshot,
            timeout,
            TimerCallback::new(
                event_callback,
                CallbackArg::Event(unsafe { NonNull::new_unchecked(Arc::into_raw(event) as _) }),
            ),
        )?;
        self.wait_queue.push(timer);
        Ok(())
    }

    pub fn notify(&self, num: usize) -> usize {
        let num = if num == 0 { usize::MAX } else { num };

        let mut cnt = 0;
        while cnt < num {
            match self.wait_queue.pop() {
                Some(timer) => {
                    if !timer.cancel() {
                        match timer.callback_arg() {
                            CallbackArg::Task(task) => {
                                let blocked = unsafe { Box::from_raw(task.as_ptr()) };
                                SCHED.unblock(Box::into_inner(blocked));
                            }
                            CallbackArg::Event(event) => {
                                let event = unsafe { Arc::from_raw(event.as_ptr()) };
                                event.notify(EVENT_SIG_ASYNC).unwrap();
                            }
                        }
                        cnt += 1;
                    }
                }
                None => break,
            }
        }
        cnt
    }
}

impl Default for WaitObject {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

fn event_callback(_: Arsc<Timer>, _: Instant, arg: CallbackArg) {
    match arg {
        CallbackArg::Task(_) => unreachable!("Non-event had been asynchronously waited"),
        CallbackArg::Event(event) => {
            let event = unsafe { Arc::from_raw(event.as_ptr()) };
            event.notify(EVENT_SIG_ASYNC).unwrap();
        }
    }
}
