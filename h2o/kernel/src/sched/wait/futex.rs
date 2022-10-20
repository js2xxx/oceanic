use core::{fmt, hash::BuildHasherDefault, intrinsics, time::Duration};

use collection_ex::{CHashMap, FnvHasher};
use sv_call::*;

use super::WaitObject;

type BH = BuildHasherDefault<FnvHasher>;
pub type FutexKey = crate::syscall::UserPtr<crate::syscall::In, u64>;
pub type FutexRef<'a> = collection_ex::CHashMapReadGuard<'a, FutexKey, Futex, BH>;
pub type Futexes = CHashMap<FutexKey, Futex, BH>;

pub struct Futex {
    key: FutexKey,
    wo: WaitObject,
}

impl fmt::Debug for Futex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Futex")
            .field("key", &self.key)
            .field("wo_len", &self.wo.wait_queue.len())
            .finish()
    }
}

impl Futex {
    #[inline]
    pub fn new(key: FutexKey) -> Self {
        Futex {
            key,
            wo: WaitObject::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.wo.wait_queue.is_empty()
    }

    fn wait<T>(this: FutexRef<'_>, guard: T, val: u64, timeout: Duration) -> Result {
        let ptr = this.key.as_ptr();
        if unsafe { intrinsics::atomic_load_seqcst(ptr) } == val {
            unsafe {
                let wo = &*(&this.wo as *const WaitObject);
                wo.wait((this, guard), timeout, "Futex::wait")
            }
        } else {
            Err(EINVAL)
        }
    }

    #[inline]
    fn wake(&self, num: usize) -> Result<usize> {
        Ok(self.wo.notify(num, false))
    }

    fn requeue(&self, other: &Self, num: usize) -> Result<usize> {
        let mut rem = num;
        while rem > 0 {
            match self.wo.wait_queue.pop() {
                None => break,
                Some(task) => {
                    other.wo.wait_queue.push(task);
                    rem -= 1;
                }
            }
        }
        Ok(num - rem)
    }
}

mod syscall {
    use sv_call::*;

    use super::Futex;
    use crate::{
        cpu::time,
        sched::{PREEMPT, SCHED},
        syscall::{In, InOut, UserPtr},
    };

    #[syscall]
    fn futex_wait(ptr: UserPtr<In, u64>, expected: u64, timeout_us: u64) -> Result {
        let _ = unsafe { ptr.read() }?;

        let pree = PREEMPT.lock();
        let futex = unsafe { (*SCHED.current()).as_ref().unwrap().space.futex(ptr) };
        let ret = Futex::wait(futex, pree, expected, time::from_us(timeout_us));

        SCHED.with_current(|cur| {
            unsafe { cur.space.try_drop_futex(ptr) };
            Ok(())
        })?;

        ret
    }

    #[syscall]
    fn futex_wake(ptr: UserPtr<In, u64>, num: usize) -> Result<usize> {
        let _ = unsafe { ptr.read() }?;
        SCHED.with_current(|cur| {
            let futex = unsafe { cur.space.futex(ptr) };
            futex.wake(num)
        })
    }

    #[syscall]
    fn futex_reque(
        ptr: UserPtr<In, u64>,
        wake_num: UserPtr<InOut, usize>,
        other: UserPtr<In, u64>,
        requeue_num: UserPtr<InOut, usize>,
    ) -> Result {
        let _ = unsafe { ptr.read() }?;
        let _ = unsafe { other.read() }?;
        let (wake, requeue) = unsafe { (wake_num.read()?, requeue_num.read()?) };

        let pree = PREEMPT.lock();
        let futex = unsafe { (*SCHED.current()).as_ref().unwrap().space.futex(ptr) };
        let other = unsafe { (*SCHED.current()).as_ref().unwrap().space.futex(other) };

        let wake = futex.wake(wake)?;
        let requeue = futex.requeue(&other, requeue)?;
        drop(pree);

        unsafe {
            wake_num.write(wake)?;
            requeue_num.write(requeue)?;
        }

        Ok(())
    }
}
