use core::ops::{Deref, DerefMut};

use archop::{IntrMutex, IntrMutexGuard};

use crate::sched::task;

pub struct PreemptMutex {
    current: IntrMutex<Option<task::Ready>>,
}

impl PreemptMutex {
    pub fn new() -> Self {
        PreemptMutex {
            current: IntrMutex::new(None),
        }
    }
    pub fn lock(&self) -> PreemptGuard {
        PreemptGuard::new(self.current.lock())
    }
}

pub struct PreemptGuard<'a> {
    current: IntrMutexGuard<'a, Option<task::Ready>>,
}

impl<'a> PreemptGuard<'a> {
    pub(super) fn new(mut current: IntrMutexGuard<'a, Option<task::Ready>>) -> Self {
        // if let Some(ref mut current) = &mut *current {
        //     current.preempt_count += 1;
        // }
        PreemptGuard { current }
    }
}

impl<'a> Deref for PreemptGuard<'a> {
    type Target = Option<task::Ready>;

    fn deref(&self) -> &Self::Target {
        &*self.current
    }
}

impl<'a> DerefMut for PreemptGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.current
    }
}

impl<'a> Drop for PreemptGuard<'a> {
    fn drop(&mut self) {
        // if let Some(ref mut current) = &mut *self.current {
        //     current.preempt_count -= 1;
        // }
    }
}
