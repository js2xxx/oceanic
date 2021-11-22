use core::ops::{Deref, DerefMut};

use spin::{Mutex, MutexGuard};

use crate::IntrState;

pub struct IntrMutexGuard<'a, T>(MutexGuard<'a, T>, IntrState);

impl<'a, T> Deref for IntrMutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, T> DerefMut for IntrMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
pub struct IntrMutex<T>(Mutex<T>);

impl<T> IntrMutex<T> {
    pub const fn new(data: T) -> Self {
        IntrMutex(Mutex::new(data))
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.0.get_mut()
    }

    pub fn into_inner(self) -> T {
        self.0.into_inner()
    }

    pub fn try_lock(&self) -> Option<IntrMutexGuard<T>> {
        let state = IntrState::lock();
        match self.0.try_lock() {
            Some(guard) => Some(IntrMutexGuard(guard, state)),
            None => {
                drop(state);
                None
            }
        }
    }

    pub fn lock(&self) -> IntrMutexGuard<T> {
        let state = IntrState::lock();
        let guard = self.0.lock();
        IntrMutexGuard(guard, state)
    }

    pub fn is_locked(&self) -> bool {
        self.0.is_locked()
    }
}
