mod cell;
mod queue;

use alloc::{boxed::Box, sync::Arc};
use core::time::Duration;

pub use cell::WaitCell;
pub use queue::WaitQueue;

use super::*;
use crate::cpu::time::Timer;

#[derive(Debug)]
pub struct WaitObject {
    pub(super) wait_queue: deque::Injector<Arc<Timer>>,
}

unsafe impl Send for WaitObject {}
unsafe impl Sync for WaitObject {}

impl WaitObject {
    pub fn new() -> Self {
        WaitObject {
            wait_queue: deque::Injector::new(),
        }
    }

    #[inline]
    pub fn wait<T>(&self, guard: T, timeout: Duration, block_desc: &'static str) -> bool {
        let timer = SCHED.block_current(guard, Some(self), timeout, block_desc);
        timer.map_or(false, |timer| !timer.is_fired())
    }

    pub fn notify(&self, num: usize) -> usize {
        let num = if num == 0 { usize::MAX } else { num };

        let mut cnt = 0;
        while cnt < num {
            match self.wait_queue.steal() {
                deque::Steal::Success(timer) => {
                    if !timer.cancel() {
                        let blocked =
                            unsafe { Box::from_raw(timer.callback_arg().cast::<task::Blocked>()) };
                        SCHED.unblock(Box::into_inner(blocked));
                        cnt += 1;
                    }
                }
                deque::Steal::Retry => {}
                deque::Steal::Empty => break,
            }
        }
        cnt
    }
}

mod syscall {
    use alloc::sync::Arc;

    use solvent::*;

    use super::*;

    #[syscall]
    fn wo_new() -> u32 {
        let wo = Arc::new(WaitObject::new());
        SCHED
            .with_current(|cur| {
                let info = cur.tid().info();
                info.handles().write().insert(wo).raw()
            })
            .map_or(Err(Error(ESRCH)), Ok)
    }

    #[syscall]
    fn wo_notify(hdl: Handle, n: usize) -> usize {
        hdl.check_null()?;
        let wo = SCHED
            .with_current(|cur| {
                let info = cur.tid().info();
                info.handles().read().get::<Arc<WaitObject>>(hdl).cloned()
            })
            .flatten()
            .ok_or(Error(EINVAL))?;
        Ok(wo.notify(n))
    }
}
