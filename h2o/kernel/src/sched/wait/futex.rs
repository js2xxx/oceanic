use core::{hash::BuildHasherDefault, intrinsics, time::Duration};

use collection_ex::{CHashMap, FnvHasher};
use paging::PAddr;
use solvent::*;
use spin::Lazy;

use super::WaitObject;

type BH = BuildHasherDefault<FnvHasher>;
type FutexRef<'a> = collection_ex::CHashMapReadGuard<'a, PAddr, Futex, BH>;
static FUTEX: Lazy<CHashMap<PAddr, Futex, BH>> = Lazy::new(|| CHashMap::new(BH::default()));

struct Futex {
    addr: PAddr,
    wo: WaitObject,
}

impl Futex {
    #[inline]
    fn get_or_insert<'a>(addr: PAddr) -> FutexRef<'a> {
        FUTEX
            .get_or_insert(
                addr,
                Futex {
                    addr,
                    wo: WaitObject::new(),
                },
            )
            .downgrade()
    }

    fn wait(&self, val: u64, timeout: Duration) -> Result<bool> {
        let ptr = self.addr.to_laddr(minfo::ID_OFFSET).cast::<u64>();
        if unsafe { intrinsics::atomic_load(ptr) } == val {
            Ok(self.wo.wait((), timeout, "Futex::wait"))
        } else {
            Err(Error::EINVAL)
        }
    }

    #[inline]
    fn wake(&self, num: usize) -> Result<usize> {
        Ok(self.wo.notify(num))
    }

    fn requeue(&self, other: &Self, num: usize) -> Result<usize> {
        let mut rem = num;
        while rem > 0 {
            match self.wo.wait_queue.steal() {
                crate::sched::deque::Steal::Empty => break,
                crate::sched::deque::Steal::Success(task) => {
                    other.wo.wait_queue.push(task);
                    rem -= 1;
                }
                crate::sched::deque::Steal::Retry => {}
            }
        }
        Ok(num - rem)
    }
}

mod syscall {
    use core::ptr::NonNull;

    use solvent::*;

    use super::*;
    use crate::{
        sched::{PREEMPT, SCHED},
        syscall::{In, InOut, UserPtr},
    };

    #[syscall]
    fn futex_wait(ptr: UserPtr<In, u64>, expected: u64, timeout_us: u64) -> bool {
        ptr.check()?;
        let timeout = if timeout_us == u64::MAX {
            Duration::MAX
        } else {
            Duration::from_micros(timeout_us)
        };

        let ptr = unsafe { NonNull::new_unchecked(ptr.as_ptr()) };
        let addr = SCHED
            .with_current(|cur| cur.space.get(ptr.cast()).map_err(Into::into))
            .ok_or(Error::ESRCH)
            .flatten()?;

        let _pree = PREEMPT.lock();
        let futex = Futex::get_or_insert(addr);
        let ret = futex.wait(expected, timeout);

        if futex.wo.wait_queue.is_empty() {
            drop(futex);
            let _ = FUTEX.remove_if(&addr, |futex| futex.wo.wait_queue.is_empty());
        }

        ret
    }

    #[syscall]
    fn futex_wake(ptr: UserPtr<In, u64>, num: usize) -> usize {
        ptr.check()?;

        let ptr = unsafe { NonNull::new_unchecked(ptr.as_ptr()) };
        let addr = SCHED
            .with_current(|cur| cur.space.get(ptr.cast()).map_err(Into::into))
            .ok_or(Error::ESRCH)
            .flatten()?;

        PREEMPT.scope(|| Futex::get_or_insert(addr).wake(num))
    }

    #[syscall]
    fn futex_requeue(
        ptr: UserPtr<In, u64>,
        wake_num: UserPtr<InOut, usize>,
        other: UserPtr<In, u64>,
        requeue_num: UserPtr<InOut, usize>,
    ) {
        ptr.check()?;
        other.check()?;
        let (wake, requeue, ptr, other) = unsafe {
            (
                wake_num.r#in().read()?,
                requeue_num.r#in().read()?,
                NonNull::new_unchecked(ptr.as_ptr()),
                NonNull::new_unchecked(other.as_ptr()),
            )
        };

        let (addr, other) = SCHED
            .with_current(|cur| {
                let space = &cur.space;
                space
                    .get(ptr.cast())
                    .and_then(|addr| space.get(other.cast()).map(|other| (addr, other)))
                    .map_err(Into::into)
            })
            .ok_or(Error::ESRCH)
            .flatten()?;

        let pree = PREEMPT.lock();
        let futex = Futex::get_or_insert(addr);
        let other = Futex::get_or_insert(other);

        let wake = futex.wake(wake)?;
        let requeue = futex.requeue(&other, requeue)?;
        drop(pree);

        unsafe {
            wake_num.out().write(wake).unwrap();
            requeue_num.out().write(requeue).unwrap();
        }

        Ok(())
    }
}
