use core::{fmt, time::Duration};

use solvent::time::Instant;

use super::{imp::RawCondvar, mutex::MutexGuard};

/// Condition variable based on Rust `std`'s implementation without poison
/// detection.
pub struct Condvar {
    inner: RawCondvar,
}

impl Condvar {
    #[inline]
    pub const fn new() -> Self {
        Condvar {
            inner: RawCondvar::new(),
        }
    }

    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        unsafe { self.inner.wait(guard.raw_mutex(), Duration::MAX) };
        guard
    }

    pub fn wait_while<'a, T, F>(
        &self,
        mut guard: MutexGuard<'a, T>,
        mut condition: F,
    ) -> MutexGuard<'a, T>
    where
        F: FnMut(&mut T) -> bool,
    {
        while condition(&mut *guard) {
            guard = self.wait(guard);
        }
        guard
    }

    pub fn wait_timeout<'a, T>(
        &self,
        guard: MutexGuard<'a, T>,
        timeout: Duration,
    ) -> MutexGuard<'a, T> {
        unsafe { self.inner.wait(guard.raw_mutex(), timeout) };
        guard
    }

    pub fn wait_timeout_while<'a, T, F>(
        &self,
        mut guard: MutexGuard<'a, T>,
        timeout: Duration,
        mut condition: F,
    ) -> (MutexGuard<'a, T>, bool)
    where
        F: FnMut(&mut T) -> bool,
    {
        let start = Instant::now();
        loop {
            if !condition(&mut *guard) {
                break (guard, false);
            }
            let time = match timeout.checked_sub(start.elapsed()) {
                Some(time) => time,
                None => break (guard, true),
            };
            guard = self.wait_timeout(guard, time);
        }
    }

    #[inline]
    pub fn notify_one(&self) -> bool {
        self.inner.notify_one()
    }

    #[inline]
    pub fn notify_all(&self) -> bool {
        self.inner.notify_all()
    }
}

impl fmt::Debug for Condvar {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Condvar").finish_non_exhaustive()
    }
}

impl Default for Condvar {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
