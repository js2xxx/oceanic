use core::{
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering::*},
    time::Duration,
};

use crossbeam::utils::Backoff;
use solvent::sync::{futex_wait, futex_wake, futex_wake_all};

pub struct RawMutex {
    futex: AtomicU64,
}

impl RawMutex {
    #[inline]
    pub const fn new() -> Self {
        RawMutex {
            futex: AtomicU64::new(0),
        }
    }

    #[inline]
    pub unsafe fn try_lock(&self) -> bool {
        self.futex.compare_exchange(0, 1, Acquire, Relaxed).is_ok()
    }

    #[inline]
    pub unsafe fn lock(&self) {
        if !self.try_lock() {
            self.lock_contended()
        }
    }

    #[cold]
    fn lock_contended(&self) {
        let backoff = Backoff::new();

        let mut state = self.spin(&backoff);
        if state == 0 {
            match self.futex.compare_exchange(0, 1, Acquire, Relaxed) {
                Ok(_) => return,
                Err(s) => state = s,
            }
        }

        loop {
            if state != 2 && self.futex.swap(2, Acquire) == 0 {
                break;
            }
            futex_wait(&self.futex, 2, Duration::MAX);
            state = self.spin(&backoff);
        }
    }

    fn spin(&self, backoff: &Backoff) -> u64 {
        loop {
            let state = self.futex.load(Relaxed);
            if state != 1 || backoff.is_completed() {
                break state;
            }
            backoff.spin();
        }
    }

    #[inline]
    pub unsafe fn unlock(&self) {
        if self.futex.swap(0, Release) == 2 {
            futex_wake(&self.futex);
        }
    }
}

pub struct RawCondvar {
    futex: AtomicU64,
}

impl RawCondvar {
    pub const fn new() -> Self {
        RawCondvar {
            futex: AtomicU64::new(0),
        }
    }

    pub fn notify_one(&self) -> bool {
        self.futex.fetch_add(1, Relaxed);
        futex_wake(&self.futex)
    }

    pub fn notify_all(&self) -> bool {
        self.futex.fetch_add(1, Relaxed);
        futex_wake_all(&self.futex)
    }

    pub unsafe fn wait(&self, mutex: &RawMutex, timeout: Duration) -> bool {
        let value = self.futex.load(Relaxed);
        mutex.unlock();
        let ret = futex_wait(&self.futex, value, timeout);
        mutex.lock();
        ret
    }
}

pub struct Parker {
    futex: AtomicU64,
}

impl Parker {
    const EMPTY: u64 = 0;
    const PARKED: u64 = u64::MAX;
    const NOTIFIED: u64 = 1;

    #[inline]
    pub const fn new() -> Self {
        Parker {
            futex: AtomicU64::new(Self::EMPTY),
        }
    }

    pub fn park(self: Pin<&Self>) {
        if self.futex.fetch_sub(1, Acquire) == Self::NOTIFIED {
            return;
        }
        loop {
            futex_wait(&self.futex, Self::PARKED, Duration::MAX);
            if self
                .futex
                .compare_exchange(Self::NOTIFIED, Self::EMPTY, Acquire, Acquire)
                .is_ok()
            {
                break;
            }
        }
    }

    pub fn park_timeout(self: Pin<&Self>, timeout: Duration) -> bool {
        if self.futex.fetch_sub(1, Acquire) == Self::NOTIFIED {
            return true;
        }
        futex_wait(&self.futex, Self::PARKED, timeout);
        self.futex.swap(Self::EMPTY, Acquire) == Self::NOTIFIED
    }

    pub fn unpark(self: Pin<&Self>) {
        if self.futex.swap(Self::NOTIFIED, Release) == Self::PARKED {
            futex_wake(&self.futex);
        }
    }
}
