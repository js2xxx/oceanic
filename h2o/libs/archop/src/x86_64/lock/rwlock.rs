use core::ops::{Deref, DerefMut};

use spin::{RwLock, RwLockReadGuard, RwLockUpgradableGuard, RwLockWriteGuard};

use crate::IntrState;

pub struct IntrRwLockReadGuard<'a, T> {
    _intr: IntrState,
    inner: RwLockReadGuard<'a, T>,
}

impl<'a, T> Deref for IntrRwLockReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

pub struct IntrRwLockWriteGuard<'a, T> {
    _intr: IntrState,
    inner: RwLockWriteGuard<'a, T>,
}

impl<'a, T> Deref for IntrRwLockWriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl<'a, T> DerefMut for IntrRwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.deref_mut()
    }
}

pub struct IntrRwLockUpgradableGuard<'a, T> {
    _intr: IntrState,
    inner: RwLockUpgradableGuard<'a, T>,
}

impl<'a, T> IntrRwLockUpgradableGuard<'a, T> {
    pub fn upgrade(self) -> IntrRwLockWriteGuard<'a, T> {
        IntrRwLockWriteGuard {
            _intr: self._intr,
            inner: self.inner.upgrade(),
        }
    }

    pub fn downgrade(self) -> IntrRwLockReadGuard<'a, T> {
        IntrRwLockReadGuard {
            _intr: self._intr,
            inner: self.inner.downgrade(),
        }
    }
}

impl<'a, T> Deref for IntrRwLockUpgradableGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

#[derive(Debug, Default)]
pub struct IntrRwLock<T>(RwLock<T>);

impl<T> IntrRwLock<T> {
    pub const fn new(o: T) -> Self {
        IntrRwLock(RwLock::new(o))
    }

    #[inline]
    pub fn into_inner(self) -> T {
        self.0.into_inner()
    }

    pub fn as_mut_ptr(&self) -> *mut T {
        self.0.as_mut_ptr()
    }

    pub fn read(&self) -> IntrRwLockReadGuard<T> {
        let flags = IntrState::lock();
        let inner = self.0.read();
        IntrRwLockReadGuard {
            _intr: flags,
            inner,
        }
    }

    pub fn write(&self) -> IntrRwLockWriteGuard<T> {
        let flags = IntrState::lock();
        let inner = self.0.write();
        IntrRwLockWriteGuard {
            _intr: flags,
            inner,
        }
    }

    pub fn upgradable_read(&self) -> IntrRwLockUpgradableGuard<T> {
        let flags = IntrState::lock();
        let inner = self.0.upgradeable_read();
        IntrRwLockUpgradableGuard {
            _intr: flags,
            inner,
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.0.get_mut()
    }
}
