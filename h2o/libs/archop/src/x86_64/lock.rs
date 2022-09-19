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

pub struct PreemptStateGuard<'a>(u64, &'a AtomicUsize);

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
        self.0.fetch_add(1, Acquire);
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

    pub fn is_locked(&self) -> bool {
        self.0.load(Acquire) > 0
    }

    pub fn raw(&self) -> usize {
        self.0.load(Acquire)
    }

    /// # Safety
    ///
    /// This function must be called only if a [`PreemptStateGuard`] is
    /// [`forget`]ed or peered with [`disable`].
    ///
    /// [`forget`]: core::mem::forget
    /// [`disable`]: Self::disable
    pub unsafe fn enable(&self, flags: Option<u64>) -> bool {
        let p = self.0.load(Acquire);
        if p > 0
            && self
                .0
                .compare_exchange_weak(p, p - 1, AcqRel, Acquire)
                .is_ok()
        {
            if p == 1 {
                crate::resume_intr(flags);
            }
            true
        } else {
            false
        }
    }

    /// # Safety
    ///
    /// This function must be called only if peered with [`enable`].
    ///
    /// [`enable`]: Self::enable
    pub unsafe fn disable(&self) -> (u64, usize) {
        let flags = crate::pause_intr();
        let old = self.0.fetch_add(1, AcqRel);
        (flags, old)
    }
}
