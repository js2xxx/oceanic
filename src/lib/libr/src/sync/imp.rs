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
        let mut state = self.spin();
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
            state = self.spin();
        }
    }

    fn spin(&self) -> u64 {
        let backoff = Backoff::new();
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

pub struct RawRwLock {
    state: AtomicU64,
    writer_notify: AtomicU64,
}

impl RawRwLock {
    const READ_LOCKED: u64 = 1;
    const MASK: u64 = (1 << 30) - 1;
    const WRITE_LOCKED: u64 = Self::MASK;
    const MAX_READERS: u64 = Self::MASK - 1;
    const READERS_WAITING: u64 = 1 << 30;
    const WRITERS_WAITING: u64 = 1 << 31;

    #[inline]
    pub const fn new() -> Self {
        RawRwLock {
            state: AtomicU64::new(0),
            writer_notify: AtomicU64::new(0),
        }
    }

    #[inline]
    fn is_unlocked(state: u64) -> bool {
        state & Self::MASK == 0
    }

    #[inline]
    fn is_write_locked(state: u64) -> bool {
        state & Self::MASK == Self::WRITE_LOCKED
    }

    #[inline]
    fn has_readers_waiting(state: u64) -> bool {
        state & Self::READERS_WAITING != 0
    }

    #[inline]
    fn has_writers_waiting(state: u64) -> bool {
        state & Self::WRITERS_WAITING != 0
    }

    #[inline]
    fn is_read_lockable(state: u64) -> bool {
        // This also returns false if the counter could overflow if we tried to read
        // lock it.
        //
        // We don't allow read-locking if there's readers waiting, even if the lock is
        // unlocked and there's no writers waiting. The only situation when this
        // happens is after unlocking, at which point the unlocking thread might
        // be waking up writers, which have priority over readers. The unlocking
        // thread will clear the readers waiting bit and wake up readers, if necessary.
        state & Self::MASK < Self::MAX_READERS
            && !Self::has_readers_waiting(state)
            && !Self::has_writers_waiting(state)
    }

    #[inline]
    fn has_reached_max_readers(state: u64) -> bool {
        state & Self::MASK == Self::MAX_READERS
    }

    #[inline]
    pub unsafe fn try_read(&self) -> bool {
        self.state
            .fetch_update(Acquire, Relaxed, |s| {
                Self::is_read_lockable(s).then_some(s + Self::READ_LOCKED)
            })
            .is_ok()
    }

    #[inline]
    pub unsafe fn read(&self) {
        let state = self.state.load(Relaxed);
        if !Self::is_read_lockable(state)
            || self
                .state
                .compare_exchange_weak(state, state + Self::READ_LOCKED, Acquire, Relaxed)
                .is_err()
        {
            self.read_contended();
        }
    }

    #[inline]
    pub unsafe fn read_unlock(&self) {
        let state = self.state.fetch_sub(Self::READ_LOCKED, Release) - Self::READ_LOCKED;

        // It's impossible for a reader to be waiting on a read-locked RwLock,
        // except if there is also a writer waiting.
        debug_assert!(!Self::has_readers_waiting(state) || Self::has_writers_waiting(state));

        // Wake up a writer if we were the last reader and there's a writer waiting.
        if Self::is_unlocked(state) && Self::has_writers_waiting(state) {
            self.wake_writer_or_readers(state);
        }
    }

    #[cold]
    fn read_contended(&self) {
        let mut state = self.spin_read();

        loop {
            // If we can lock it, lock it.
            if Self::is_read_lockable(state) {
                match self.state.compare_exchange_weak(
                    state,
                    state + Self::READ_LOCKED,
                    Acquire,
                    Relaxed,
                ) {
                    Ok(_) => return, // Locked!
                    Err(s) => {
                        state = s;
                        continue;
                    }
                }
            }

            // Check for overflow.
            if Self::has_reached_max_readers(state) {
                panic!("too many active read locks on RwLock");
            }

            // Make sure the readers waiting bit is set before we go to sleep.
            if !Self::has_readers_waiting(state) {
                if let Err(s) = self.state.compare_exchange(
                    state,
                    state | Self::READERS_WAITING,
                    Relaxed,
                    Relaxed,
                ) {
                    state = s;
                    continue;
                }
            }

            // Wait for the state to change.
            futex_wait(&self.state, state | Self::READERS_WAITING, Duration::MAX);

            // Spin again after waking up.
            state = self.spin_read();
        }
    }

    #[inline]
    pub unsafe fn try_write(&self) -> bool {
        self.state
            .fetch_update(Acquire, Relaxed, |s| {
                Self::is_unlocked(s).then_some(s + Self::WRITE_LOCKED)
            })
            .is_ok()
    }

    #[inline]
    pub unsafe fn write(&self) {
        if self
            .state
            .compare_exchange_weak(0, Self::WRITE_LOCKED, Acquire, Relaxed)
            .is_err()
        {
            self.write_contended();
        }
    }

    #[inline]
    pub unsafe fn write_unlock(&self) {
        let state = self.state.fetch_sub(Self::WRITE_LOCKED, Release) - Self::WRITE_LOCKED;

        debug_assert!(Self::is_unlocked(state));

        if Self::has_writers_waiting(state) || Self::has_readers_waiting(state) {
            self.wake_writer_or_readers(state);
        }
    }

    #[cold]
    fn write_contended(&self) {
        let mut state = self.spin_write();

        let mut other_writers_waiting = 0;

        loop {
            // If it's unlocked, we try to lock it.
            if Self::is_unlocked(state) {
                match self.state.compare_exchange_weak(
                    state,
                    state | Self::WRITE_LOCKED | other_writers_waiting,
                    Acquire,
                    Relaxed,
                ) {
                    Ok(_) => return, // Locked!
                    Err(s) => {
                        state = s;
                        continue;
                    }
                }
            }

            // Set the waiting bit indicating that we're waiting on it.
            if !Self::has_writers_waiting(state) {
                if let Err(s) = self.state.compare_exchange(
                    state,
                    state | Self::WRITERS_WAITING,
                    Relaxed,
                    Relaxed,
                ) {
                    state = s;
                    continue;
                }
            }

            // Other writers might be waiting now too, so we should make sure
            // we keep that bit on once we manage lock it.
            other_writers_waiting = Self::WRITERS_WAITING;

            // Examine the notification counter before we check if `state` has changed,
            // to make sure we don't miss any notifications.
            let seq = self.writer_notify.load(Acquire);

            // Don't go to sleep if the lock has become available,
            // or if the writers waiting bit is no longer set.
            state = self.state.load(Relaxed);
            if Self::is_unlocked(state) || !Self::has_writers_waiting(state) {
                continue;
            }

            // Wait for the state to change.
            futex_wait(&self.writer_notify, seq, Duration::MAX);

            // Spin again after waking up.
            state = self.spin_write();
        }
    }

    /// Wake up waiting threads after unlocking.
    ///
    /// If both are waiting, this will wake up only one writer, but will fall
    /// back to waking up readers if there was no writer to wake up.
    #[cold]
    fn wake_writer_or_readers(&self, mut state: u64) {
        assert!(Self::is_unlocked(state));

        // The readers waiting bit might be turned on at any point now,
        // since readers will block when there's anything waiting.
        // Writers will just lock the lock though, regardless of the waiting bits,
        // so we don't have to worry about the writer waiting bit.
        //
        // If the lock gets locked in the meantime, we don't have to do
        // anything, because then the thread that locked the lock will take
        // care of waking up waiters when it unlocks.

        // If only writers are waiting, wake one of them up.
        if state == Self::WRITERS_WAITING {
            match self.state.compare_exchange(state, 0, Relaxed, Relaxed) {
                Ok(_) => {
                    self.wake_writer();
                    return;
                }
                Err(s) => {
                    // Maybe some readers are now waiting too. So, continue to the next `if`.
                    state = s;
                }
            }
        }

        // If both writers and readers are waiting, leave the readers waiting
        // and only wake up one writer.
        if state == Self::READERS_WAITING + Self::WRITERS_WAITING {
            if self
                .state
                .compare_exchange(state, Self::READERS_WAITING, Relaxed, Relaxed)
                .is_err()
            {
                // The lock got locked. Not our problem anymore.
                return;
            }
            if self.wake_writer() {
                return;
            }
            // No writers were actually blocked on futex_wait, so we continue
            // to wake up readers instead, since we can't be sure if we notified a writer.
            state = Self::READERS_WAITING;
        }

        // If readers are waiting, wake them all up.
        if state == Self::READERS_WAITING
            && self
                .state
                .compare_exchange(state, 0, Relaxed, Relaxed)
                .is_ok()
        {
            futex_wake_all(&self.state);
        }
    }

    /// This wakes one writer and returns true if we woke up a writer that was
    /// blocked on futex_wait.
    ///
    /// If this returns false, it might still be the case that we notified a
    /// writer that was about to go to sleep.
    fn wake_writer(&self) -> bool {
        self.writer_notify.fetch_add(1, Release);
        futex_wake(&self.writer_notify)
        // Note that FreeBSD and DragonFlyBSD don't tell us whether they woke
        // up any threads or not, and always return `false` here. That still
        // results in correct behaviour: it just means readers get woken up as
        // well in case both readers and writers were waiting.
    }

    #[inline]
    fn spin_until(&self, f: impl Fn(u64) -> bool) -> u64 {
        let backoff = Backoff::new();
        loop {
            let state = self.state.load(Relaxed);
            if f(state) || backoff.is_completed() {
                return state;
            }
            backoff.spin();
        }
    }

    #[inline]
    fn spin_write(&self) -> u64 {
        // Stop spinning when it's unlocked or when there's waiting writers, to keep
        // things somewhat fair.
        self.spin_until(|state| Self::is_unlocked(state) || Self::has_writers_waiting(state))
    }

    #[inline]
    fn spin_read(&self) -> u64 {
        // Stop spinning when it's unlocked or read locked, or when there's waiting
        // threads.
        self.spin_until(|state| {
            !Self::is_write_locked(state)
                || Self::has_readers_waiting(state)
                || Self::has_writers_waiting(state)
        })
    }
}
