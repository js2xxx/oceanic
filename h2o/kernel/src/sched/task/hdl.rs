mod node;

use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use core::{
    any::Any,
    hash::BuildHasherDefault,
    mem,
    ops::Deref,
    ptr::NonNull,
    sync::atomic::{AtomicU32, Ordering::SeqCst},
};

use collection_ex::{CHashMap, FnvHasher};
use sv_call::{Feature, Result, EINVAL, ETYPE};

pub use self::node::{Ref, MAX_HANDLE_COUNT};
use crate::sched::{ipc::Channel, Event, PREEMPT};

type BH = BuildHasherDefault<FnvHasher>;

pub unsafe trait DefaultFeature: Any + Send + Sync {
    fn default_features() -> Feature;
}

unsafe impl<T: DefaultFeature + ?Sized> DefaultFeature for crate::sched::Arsc<T> {
    fn default_features() -> Feature {
        T::default_features()
    }
}

unsafe impl<T: DefaultFeature + ?Sized> DefaultFeature for alloc::sync::Arc<T> {
    fn default_features() -> Feature {
        T::default_features()
    }
}

pub struct RefGuard<'a, T: ?Sized + 'a> {
    _guard: collection_ex::CHashMapReadGuard<'a, u32, Ref, BH>,
    _handle: sv_call::Handle,
    object: NonNull<Ref<T>>,
}

impl<'a, T: ?Sized + 'a> Deref for RefGuard<'a, T> {
    type Target = Ref<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { self.object.as_ref() }
    }
}

#[derive(Debug)]
pub struct HandleMap {
    list: CHashMap<u32, Ref, BH>,
    mix: u32,
    next_id: AtomicU32,
}

impl HandleMap {
    #[inline]
    pub fn new() -> Self {
        HandleMap {
            list: CHashMap::default(),
            mix: archop::rand::get() as u32,
            next_id: AtomicU32::new(1),
        }
    }

    fn decode(&self, handle: sv_call::Handle) -> u32 {
        handle.raw() ^ self.mix
    }

    #[inline]
    pub fn get_ref(&self, handle: sv_call::Handle) -> Result<RefGuard<'_, dyn Any + Send + Sync>> {
        let key = self.decode(handle);
        self.list.get(&key).ok_or(EINVAL).map(|guard| {
            let object = NonNull::from(&guard as &Ref);
            RefGuard {
                _guard: guard,
                _handle: handle,
                object,
            }
        })
    }

    #[inline]
    pub fn get<T: Send + Any>(&self, handle: sv_call::Handle) -> Result<RefGuard<'_, T>> {
        let key = self.decode(handle);
        self.list.get(&key).ok_or(EINVAL).and_then(|guard| {
            if guard.is::<T>() {
                let object = NonNull::from(guard.downcast_ref::<T>().unwrap());
                Ok(RefGuard {
                    _guard: guard,
                    _handle: handle,
                    object,
                })
            } else {
                Err(ETYPE)
            }
        })
    }

    #[inline]
    pub fn clone_ref(&self, handle: sv_call::Handle) -> Result<sv_call::Handle> {
        let old = self.get_ref(handle)?;
        let new = old.try_clone()?;
        drop(old);
        self.insert_ref(new)
    }

    #[inline]
    pub fn insert_ref(&self, value: Ref) -> Result<sv_call::Handle> {
        let key = self.next_id.fetch_add(1, SeqCst);
        let old = PREEMPT.scope(|| self.list.insert(key, value));
        assert!(old.is_none());
        Ok(sv_call::Handle::new(key ^ self.mix))
    }

    #[inline]
    pub fn insert_raw<T: DefaultFeature>(
        &self,
        obj: Arc<T>,
        event: Option<Weak<dyn Event>>,
    ) -> Result<sv_call::Handle> {
        self.insert_ref(Ref::from_raw(obj, event)?)
    }

    /// # Safety
    ///
    /// The caller must ensure that `T` is [`Send`] if `send` and [`Sync`] if
    /// `sync`.
    pub unsafe fn insert_unchecked<T: Send + Sync + 'static>(
        &self,
        data: T,
        feat: Feature,
        event: Option<Weak<dyn Event>>,
    ) -> Result<sv_call::Handle> {
        // SAFETY: The safety condition is guaranteed by the caller.
        let value = unsafe { Ref::try_new_unchecked(data, feat, event) }?;
        self.insert_ref(value)
    }

    /// # Safety
    ///
    /// The caller must ensure that `T` is [`Send`] if `send` and [`Sync`] if
    /// `sync`.
    pub unsafe fn insert_raw_unchecked<T: Send + Sync + 'static>(
        &self,
        data: Arc<T>,
        feat: Feature,
        event: Option<Weak<dyn Event>>,
    ) -> Result<sv_call::Handle> {
        // SAFETY: The safety condition is guaranteed by the caller.
        let value = unsafe { Ref::from_raw_unchecked(data, feat, event) }?;
        self.insert_ref(value)
    }

    #[inline]
    pub fn insert<T: DefaultFeature>(
        &self,
        data: T,
        event: Option<Weak<dyn Event>>,
    ) -> Result<sv_call::Handle> {
        unsafe { self.insert_unchecked(data, T::default_features(), event) }
    }

    #[inline]
    pub fn remove_ref(&self, handle: sv_call::Handle) -> Result<Ref> {
        let key = self.decode(handle);
        PREEMPT.scope(|| self.list.remove(&key).ok_or(EINVAL))
    }

    pub fn remove<T: Send + Sync + Any>(&self, handle: sv_call::Handle) -> Result<Ref<T>> {
        let key = self.decode(handle);
        let res = self
            .list
            .try_remove(&key, |obj| if obj.is::<T>() { Ok(()) } else { Err(ETYPE) });
        res.map_err(|err| err.unwrap_or(EINVAL))
            .map(|obj| obj.downcast().unwrap())
    }

    fn merge(&self, objects: Vec<Ref>) -> impl Iterator<Item = Result<sv_call::Handle>> + '_ {
        objects.into_iter().map(|obj| self.insert_ref(obj))
    }

    fn split(&self, handles: &[sv_call::Handle], src: &Channel) -> Result<Vec<Ref>> {
        let mut result = Vec::with_capacity(handles.len());
        for handle in handles.iter().copied() {
            let key = self.decode(handle);
            let res = self
                .list
                .try_remove(&key, |value| match value.downcast_ref::<Channel>() {
                    Ok(chan) if chan.peer_eq(src) => Err(sv_call::EPERM),
                    Err(_) if !value.features().contains(Feature::SEND) => Err(sv_call::EPERM),
                    _ => Ok(()),
                });
            match res.map_err(|err| err.unwrap_or(EINVAL)) {
                Ok(obj) => result.push(obj),
                Err(err) => {
                    self.merge(result).for_each(drop);
                    return Err(err);
                }
            }
        }
        Ok(result)
    }

    pub fn send(&self, handles: &[sv_call::Handle], src: &Channel) -> Result<Vec<Ref>> {
        if handles.is_empty() {
            return Ok(Vec::new());
        }
        PREEMPT.scope(|| self.split(handles, src))
    }

    #[inline]
    pub fn receive(&self, other: &mut Vec<Ref>, handles: &mut [sv_call::Handle]) {
        PREEMPT.scope(|| {
            for (hdl, obj) in handles.iter_mut().zip(self.merge(mem::take(other))) {
                *hdl = obj.unwrap();
            }
        })
    }
}

impl Default for HandleMap {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

mod syscall {
    use sv_call::*;

    use crate::{
        sched::SCHED,
        syscall::{InOut, UserPtr},
    };

    #[syscall]
    fn obj_clone(hdl: Handle) -> Result<Handle> {
        hdl.check_null()?;
        SCHED.with_current(|cur| cur.space().handles().clone_ref(hdl))
    }

    #[syscall]
    fn obj_feat(hdl_ptr: UserPtr<InOut, Handle>, feat: Feature) -> Result {
        let old = unsafe { hdl_ptr.read() }?;
        old.check_null()?;
        let mut obj = SCHED.with_current(|cur| cur.space().handles().remove_ref(old))?;
        let ret = obj.set_features(feat);
        let new = SCHED.with_current(|cur| cur.space().handles().insert_ref(obj))?;
        unsafe { hdl_ptr.write(new) }?;
        ret
    }

    #[syscall]
    fn obj_drop(hdl: Handle) -> Result {
        hdl.check_null()?;
        SCHED
            .with_current(|cur| cur.space().handles().remove_ref(hdl))
            .map(|_| {})
    }
}
