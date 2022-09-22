use core::{
    mem,
    sync::atomic::{AtomicUsize, Ordering::*},
};

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

pub struct PreemptStateGuard<'a>(u64, &'a AtomicUsize);

impl<'a> PreemptStateGuard<'a> {
    #[inline]
    pub fn into_raw(self) -> (usize, u64) {
        let ret = (self.1.load(Relaxed), self.0);
        mem::forget(self);
        ret
    }
}

impl<'a> Drop for PreemptStateGuard<'a> {
    fn drop(&mut self) {
        if self.1.fetch_sub(1, Release) == 1 {
            unsafe { crate::resume_intr(Some(self.0)) };
        }
    }
}

#[repr(transparent)]
pub struct PreemptState(AtomicUsize);

impl PreemptState {
    pub const fn new() -> Self {
        PreemptState(AtomicUsize::new(0))
    }

    pub fn lock(&self) -> PreemptStateGuard {
        let flags = unsafe { crate::pause_intr() };
        self.0.fetch_add(1, Relaxed);
        PreemptStateGuard(flags, &self.0)
    }

    #[inline]
    #[track_caller]
    pub fn scope<F, R>(&self, func: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _pree = self.lock();
        func()
    }

    #[inline]
    pub fn is_locked(&self) -> bool {
        self.0.load(Relaxed) > 0
    }

    #[inline]
    pub fn raw(&self) -> usize {
        self.0.load(Relaxed)
    }

    /// # Safety
    ///
    /// `value` and `flags` must be from [`into_raw`] method and the current
    /// state must be valid.
    ///
    /// [`into_raw`]: PreemptStateGuard::into_raw
    #[inline]
    pub unsafe fn from_raw(&self, value: usize, flags: u64) -> PreemptStateGuard {
        self.0.store(if value > 0 { value } else { 1 }, Release);
        let flags = if flags > 0 {
            flags
        } else {
            crate::pause_intr()
        };
        PreemptStateGuard(flags, &self.0)
    }
}
