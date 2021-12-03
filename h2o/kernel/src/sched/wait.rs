pub mod cell;
pub mod queue;

pub use cell::WaitCell;

use super::*;
use crate::cpu::time::Instant;

#[derive(Debug)]
pub struct WaitObject {
    pub(super) wait_queue: deque::Injector<task::Blocked>,
}

impl WaitObject {
    pub fn new() -> Self {
        WaitObject {
            wait_queue: deque::Injector::new(),
        }
    }

    #[inline]
    pub fn wait<T>(&self, guard: T, block_desc: &'static str) {
        SCHED.block_current(Instant::now(), guard, self, block_desc);
    }

    pub fn notify(&self, num: Option<usize>) -> usize {
        let num = num.unwrap_or(usize::MAX);

        let mut cnt = 0;
        while cnt < num {
            match self.wait_queue.steal() {
                deque::Steal::Success(task) => {
                    SCHED.unblock(task);
                    cnt += 1;
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
    use core::num::NonZeroUsize;

    use solvent::*;

    use super::*;

    #[syscall]
    fn wo_create() -> u32 {
        let wo = Arc::new(WaitObject::new());
        SCHED
            .with_current(|cur| {
                let mut info = cur.tid().info().write();
                info.handles.insert(wo).unwrap().raw()
            })
            .map_or(Err(Error(ESRCH)), Ok)
    }

    #[syscall]
    fn wo_notify(hdl: Handle, n: usize) -> usize {
        hdl.check_null()?;
        let wo = SCHED
            .with_current(|cur| {
                let info = cur.tid().info().read();
                info.handles.get::<Arc<WaitObject>>(hdl).cloned()
            })
            .flatten()
            .ok_or(Error(EINVAL))?;
        Ok(wo.notify(NonZeroUsize::new(n).map(Into::into)))
    }
}
