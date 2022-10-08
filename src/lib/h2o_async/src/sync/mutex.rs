use alloc::vec::Vec;
use core::{
    cell::UnsafeCell,
    future::poll_fn,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering::*},
    task::{Context, Poll, Waker},
};

use solvent_std::sync::{Arsc, Mutex as SyncMutex};

pub struct Mutex<T: ?Sized>(Arsc<Inner<T>>);

unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    pub fn new(data: T) -> Self {
        Mutex(Arsc::new(Inner {
            locked: AtomicBool::new(false),
            wakers: SyncMutex::new(Vec::new()),
            data: UnsafeCell::new(data),
        }))
    }

    pub fn try_unwrap(this: Self) -> Result<T, Self> {
        match Arsc::try_unwrap(this.0) {
            Ok(inner) => {
                if inner.locked.load(Acquire) {
                    Err(Mutex(Arsc::new(inner)))
                } else {
                    Ok(inner.data.into_inner())
                }
            }
            Err(arsc) => Err(Mutex(arsc)),
        }
    }
}

impl<T: ?Sized> Mutex<T> {
    fn poll_lock(&self, cx: &mut Context<'_>) -> Poll<MutexGuard<T>> {
        if self.0.locked.swap(true, Acquire) {
            let mut list = self.0.wakers.lock();
            if self.0.locked.swap(true, Acquire) {
                if list.iter().all(|w| !w.will_wake(cx.waker())) {
                    list.push(cx.waker().clone());
                }
                return Poll::Pending;
            }
        }
        Poll::Ready(MutexGuard(self.0.clone()))
    }

    pub async fn lock(&self) -> MutexGuard<T> {
        poll_fn(|cx| self.poll_lock(cx)).await
    }

    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        (!self.0.locked.swap(true, Acquire)).then(|| MutexGuard(self.0.clone()))
    }

    pub fn is_locked(&self) -> bool {
        self.0.locked.load(Acquire)
    }
}

struct Inner<T: ?Sized> {
    locked: AtomicBool,
    wakers: SyncMutex<Vec<Waker>>,
    data: UnsafeCell<T>,
}

pub struct MutexGuard<T: ?Sized>(Arsc<Inner<T>>);

unsafe impl<T: ?Sized + Send> Send for MutexGuard<T> {}
unsafe impl<T: ?Sized + Sync> Sync for MutexGuard<T> {}

impl<T: ?Sized> Drop for MutexGuard<T> {
    fn drop(&mut self) {
        self.0.locked.store(false, Release);
        self.0.wakers.lock().drain(..).for_each(Waker::wake);
    }
}

impl<T: ?Sized> Deref for MutexGuard<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.0.data.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.0.data.get() }
    }
}
