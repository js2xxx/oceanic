//! Generic support for building blocking abstractions.

use alloc::sync::Arc;
use core::{
    marker::PhantomData,
    sync::atomic::{AtomicBool, Ordering},
};

use solvent::time::Instant;

use crate::thread::{self, Thread};

struct Inner {
    thread: Thread,
    woken: AtomicBool,
}

unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

#[derive(Clone)]
pub struct SignalToken {
    inner: Arc<Inner>,
}

pub struct WaitToken {
    inner: Arc<Inner>,
    _marker: PhantomData<*mut ()>,
}

pub fn tokens() -> (WaitToken, SignalToken) {
    let inner = Arc::new(Inner {
        thread: thread::current(),
        woken: AtomicBool::new(false),
    });
    let wait_token = WaitToken {
        inner: inner.clone(),
        _marker: PhantomData,
    };
    let signal_token = SignalToken { inner };
    (wait_token, signal_token)
}

impl SignalToken {
    pub fn signal(&self) -> bool {
        let wake = self
            .inner
            .woken
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok();
        if wake {
            self.inner.thread.unpark();
        }
        wake
    }

    /// Converts to an unsafe raw pointer. Useful for storing in a pipe's state
    /// flag.
    #[inline]
    pub unsafe fn into_raw(self) -> *mut u8 {
        Arc::into_raw(self.inner) as *mut u8
    }

    /// Converts from an unsafe raw pointer. Useful for retrieving a pipe's
    /// state flag.
    #[inline]
    pub unsafe fn from_raw(signal_ptr: *mut u8) -> SignalToken {
        SignalToken {
            inner: Arc::from_raw(signal_ptr as *mut Inner),
        }
    }
}

impl WaitToken {
    pub fn wait(self) {
        while !self.inner.woken.load(Ordering::SeqCst) {
            thread::park()
        }
    }

    /// Returns `true` if we wake up normally.
    pub fn wait_max_until(self, end: Instant) -> bool {
        while !self.inner.woken.load(Ordering::SeqCst) {
            let now = Instant::now();
            if now >= end {
                return false;
            }
            thread::park_timeout(end - now);
        }
        true
    }
}
