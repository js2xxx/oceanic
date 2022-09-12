use core::{
    cell::UnsafeCell,
    fmt,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use super::imp::RawMutex;

/// Mutual exclusion based on Rust `std`'s implementation without poison
/// detection.
pub struct Mutex<T: ?Sized> {
    inner: RawMutex,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

pub struct MutexGuard<'a, T: ?Sized> {
    lock: &'a Mutex<T>,
    _marker: PhantomData<*const T>,
}

unsafe impl<'a, T: ?Sized + Sync> Sync for MutexGuard<'a, T> {}

impl<T> Mutex<T> {
    #[inline]
    pub const fn new(data: T) -> Mutex<T> {
        Mutex {
            inner: RawMutex::new(),
            data: UnsafeCell::new(data),
        }
    }
}

impl<T: ?Sized> Mutex<T> {
    pub fn lock(&self) -> MutexGuard<'_, T> {
        unsafe {
            self.inner.lock();
            MutexGuard {
                lock: self,
                _marker: PhantomData,
            }
        }
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        unsafe {
            self.inner.try_lock().then_some(MutexGuard {
                lock: self,
                _marker: PhantomData,
            })
        }
    }
}

impl<'a, T: ?Sized> MutexGuard<'a, T> {
    #[inline]
    pub(crate) fn raw_mutex(&self) -> &RawMutex {
        &self.lock.inner
    }
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        unsafe { self.lock.inner.unlock() }
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_struct("Mutex");
        match self.try_lock() {
            Some(guard) => f.field("data", &&*guard),
            None => {
                struct G;
                impl fmt::Debug for G {
                    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("<locked>")
                    }
                }
                f.field("data", &G)
            }
        }
        .finish_non_exhaustive()
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for MutexGuard<'_, T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}
