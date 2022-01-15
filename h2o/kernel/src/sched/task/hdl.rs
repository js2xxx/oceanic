use alloc::vec::Vec;
use core::{
    hash::BuildHasherDefault,
    ops::Deref,
    sync::atomic::{AtomicU32, Ordering::SeqCst},
};

use collection_ex::{CHashMap, CHashMapReadGuard, FnvHasher};
use solvent::Handle;

use crate::sched::{
    ipc::{Channel, Object},
    PREEMPT,
};

type BH = BuildHasherDefault<FnvHasher>;

pub struct HandleGuard<'a, T> {
    _inner: CHashMapReadGuard<'a, Handle, Object, BH>,
    value: &'a T,
}

impl<'a, T> Deref for HandleGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.value }
    }
}

#[derive(Debug)]
pub struct HandleMap {
    next_id: AtomicU32,
    map: CHashMap<Handle, Object, BH>,
}

unsafe impl Send for HandleMap {}
unsafe impl Sync for HandleMap {}

impl HandleMap {
    #[inline]
    pub fn new() -> Self {
        HandleMap {
            next_id: AtomicU32::new(1),
            map: CHashMap::new(BH::default()),
        }
    }

    #[inline]
    pub fn insert<T: Send + 'static>(&self, obj: T) -> Handle {
        unsafe { self.insert_unchecked(obj, true, false) }
    }

    #[inline]
    pub fn insert_shared<T: Send + Sync + 'static>(&self, obj: T) -> Handle {
        unsafe { self.insert_unchecked(obj, true, true) }
    }

    /// # Safety
    ///
    /// The caller is responsible for the usage of the inserted object if its
    /// `!Send`.
    #[inline]
    pub unsafe fn insert_unchecked<T: 'static>(&self, obj: T, send: bool, shared: bool) -> Handle {
        self.insert_impl(Object::new_unchecked(obj, send, shared))
    }

    unsafe fn insert_impl(&self, obj: Object) -> Handle {
        let id = Handle::new(self.next_id.fetch_add(1, SeqCst));
        let _ret = PREEMPT.scope(|| self.map.insert(id, obj));
        debug_assert!(_ret.is_none());
        id
    }

    #[inline]
    pub fn get<T: Send + 'static>(&self, hdl: Handle) -> Option<HandleGuard<'_, T>> {
        unsafe { self.get_unchecked(hdl) }
    }

    /// # Safety
    ///
    /// The caller must ensure the current task is the owner of the
    /// [`HandleMap`] if the returned object is `!Send`.
    pub unsafe fn get_unchecked<T: 'static>(&self, hdl: Handle) -> Option<HandleGuard<'_, T>> {
        self.map.get(&hdl).and_then(|inner| {
            match inner.deref_unchecked().map(|value| value as *const _) {
                Some(value) => Some(HandleGuard {
                    _inner: inner,
                    value: unsafe { &*value },
                }),
                None => None,
            }
        })
    }

    pub fn clone_handle(&self, hdl: Handle) -> Option<Handle> {
        match self.map.get(&hdl).and_then(|k| Object::clone(&*k)) {
            Some(o) => Some(unsafe { self.insert_impl(o) }),
            None => None,
        }
    }

    pub fn remove<T: Send + 'static>(&self, hdl: Handle) -> Option<T> {
        self.map
            .remove_if(&hdl, |obj| obj.deref::<T>().is_some())
            .map(|obj| Object::into_inner(obj).unwrap())
    }

    pub fn drop_shared<T: Send + Sync + 'static>(&self, hdl: Handle) -> bool {
        self.map
            .remove_if(&hdl, |obj| obj.deref::<T>().is_some())
            .is_some()
    }

    /// # Safety
    ///
    /// The caller must ensure the current task is the owner of the
    /// [`HandleMap`] if the dropped object is `!Send`.
    pub unsafe fn drop_unchecked(&self, hdl: Handle) -> bool {
        self.map.remove(&hdl).is_some()
    }

    /// # Safety
    ///
    /// The caller must ensure every object indexed by `handles` in the map is
    /// `Send`, excluding:
    /// * [`crate::mem::space::Virt`].
    pub unsafe fn send_for_channel<'a, 'b>(
        &'a self,
        handles: &'b [Handle],
        chan: Handle,
    ) -> Result<(Vec<Object>, HandleGuard<'a, Channel>), solvent::Error> {
        debug_assert!(!handles.contains(&chan));

        let get_chan = || {
            self.get::<Channel>(chan)
                .ok_or(solvent::Error(solvent::EINVAL))
        };

        {
            let chan = get_chan()?;
            for hdl in handles {
                match self.map.get(hdl) {
                    None => return Err(solvent::Error(solvent::EINVAL)),
                    Some(obj) if !obj.is_send() => return Err(solvent::Error(solvent::EPERM)),
                    Some(obj) => match (*obj).deref::<Channel>() {
                        Some(other) if unsafe { &*chan }.is_peer(other) => {
                            return Err(solvent::Error(solvent::EPERM))
                        }
                        _ => {}
                    },
                }
            }
        }
        let obj = handles
            .iter()
            .map(|hdl| self.map.remove(hdl).unwrap())
            .collect();
        Ok((obj, get_chan().unwrap()))
    }

    pub fn receive(&self, objects: Vec<Object>) -> Vec<Handle> {
        objects
            .into_iter()
            .map(|obj| unsafe { self.insert_impl(obj) })
            .collect()
    }
}

impl Default for HandleMap {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

mod syscall {
    use solvent::*;

    use crate::sched::SCHED;

    #[syscall]
    fn obj_clone(hdl: Handle) -> Handle {
        hdl.check_null()?;
        SCHED
            .with_current(|cur| cur.tid().handles().clone_handle(hdl))
            .ok_or(Error(ESRCH))
            .transpose()
            .ok_or(Error(EINVAL))
            .flatten()
    }

    #[syscall]
    fn obj_drop(hdl: Handle) {
        hdl.check_null()?;
        let ret = SCHED
            .with_current(|cur| unsafe { cur.tid().handles().drop_unchecked(hdl) })
            .ok_or(Error(ESRCH))?;
        if ret {
            Ok(())
        } else {
            Err(Error(EINVAL))
        }
    }
}
