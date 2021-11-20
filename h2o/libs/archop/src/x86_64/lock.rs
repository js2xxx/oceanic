use core::sync::atomic::{AtomicUsize, Ordering::*};

pub mod mutex;
pub mod rwlock;

pub struct IntrState(u64);

impl IntrState {
    pub fn lock() -> Self {
        IntrState(unsafe { crate::pause_intr() })
    }
}

impl Drop for IntrState {
    fn drop(&mut self) {
        let data = self.0;
        unsafe { crate::resume_intr(Some(data)) };
    }
}

pub struct PreemptLockGuard<'a>(u64, &'a AtomicUsize);

impl<'a> Drop for PreemptLockGuard<'a> {
    fn drop(&mut self) {
        if self.1.fetch_sub(1, Release) == 1 {
            unsafe { crate::resume_intr(Some(self.0)) };
        }
    }
}

#[repr(transparent)]
pub struct PreemptLock(AtomicUsize);

impl PreemptLock {
    pub const fn new() -> Self {
        PreemptLock(AtomicUsize::new(0))
    }

    pub fn lock(&self) -> PreemptLockGuard {
        let flags = unsafe { crate::pause_intr() };
        self.0.fetch_add(1, Acquire);
        PreemptLockGuard(flags, &self.0)
    }

    pub fn is_locked(&self) -> bool {
        self.0.load(Acquire) > 0
    }
}
