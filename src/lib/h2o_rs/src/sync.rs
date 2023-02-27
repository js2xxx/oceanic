use core::{sync::atomic::AtomicU64, time::Duration};

use sv_call::ETIME;

pub fn futex_wait(futex: &AtomicU64, expected: u64, timeout: Duration) -> bool {
    let timeout = crate::time::try_into_us(timeout).unwrap();
    let ret = unsafe { sv_call::sv_futex_wait(futex.as_ptr(), expected, timeout) };
    !matches!(ret.into_res(), Err(ETIME))
}

pub fn futex_wake(futex: &AtomicU64) -> bool {
    let ret = unsafe { sv_call::sv_futex_wake(futex.as_ptr(), 1) };
    matches!(ret.into_res(), Ok(1))
}

pub fn futex_wake_some(futex: &AtomicU64, num: usize) -> crate::error::Result<usize> {
    let ret = unsafe { sv_call::sv_futex_wake(futex.as_ptr(), num) };
    ret.into_res().map(|num| num as usize)
}

pub fn futex_wake_all(futex: &AtomicU64) -> bool {
    let ret = unsafe { sv_call::sv_futex_wake(futex.as_ptr(), usize::MAX) };
    matches!(ret.into_res(), Ok(_))
}

pub fn futex_requeue(
    futex: &AtomicU64,
    mut wake_num: usize,
    other: &AtomicU64,
    mut requeue_num: usize,
) -> crate::error::Result<(usize, usize)> {
    let ret = unsafe {
        sv_call::sv_futex_reque(
            futex.as_ptr(),
            &mut wake_num,
            other.as_ptr(),
            &mut requeue_num,
        )
    };
    ret.into_res().map(|_| (wake_num, requeue_num))
}
