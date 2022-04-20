mod node;

use alloc::sync::{Weak, Arc};
use core::{any::Any, pin::Pin, ptr::NonNull};

use archop::Azy;
use modular_bitfield::prelude::*;
use spin::Mutex;
use sv_call::{Feature, Result};

pub use self::node::{List, Ptr, Ref, MAX_HANDLE_COUNT};
use crate::sched::{ipc::Channel, Event, PREEMPT};

#[bitfield]
struct Value {
    gen: B14,
    index: B18,
}

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

#[derive(Debug)]
pub struct HandleMap {
    list: Mutex<node::List>,
    mix: u32,
}

impl HandleMap {
    #[inline]
    pub fn new() -> Self {
        HandleMap {
            list: Mutex::new(List::new()),
            mix: archop::rand::get() as u32,
        }
    }

    fn decode(&self, handle: sv_call::Handle) -> Result<Ptr> {
        let value = handle.raw() ^ self.mix;
        let value = Value::from_bytes(value.to_ne_bytes());
        let _ = value.gen();
        usize::try_from(value.index())
            .map_err(Into::into)
            .and_then(node::decode)
    }

    fn encode(&self, value: Ptr) -> Result<sv_call::Handle> {
        let index =
            node::encode(value).and_then(|index| u32::try_from(index).map_err(Into::into))?;
        let value = Value::new()
            .with_gen(0)
            .with_index_checked(index)
            .map_err(|_| sv_call::ERANGE)?;
        Ok(sv_call::Handle::new(
            u32::from_ne_bytes(value.into_bytes()) ^ self.mix,
        ))
    }

    #[inline]
    pub fn get_ref(&self, handle: sv_call::Handle) -> Result<Pin<&Ref>> {
        self.decode(handle)
            // SAFETY: Dereference within the available range and the 
            // reference is at a fixed address.
            .map(|ptr| unsafe { Pin::new_unchecked(ptr.as_ref()) })
    }

    #[inline]
    pub fn get<T: Send + Any>(&self, handle: sv_call::Handle) -> Result<Pin<&Ref<T>>> {
        self.decode(handle)
            // SAFETY: Dereference within the available range.
            .and_then(|ptr| unsafe { ptr.as_ref().downcast_ref::<T>() })
            // SAFETY: The reference is at a fixed address.
            .map(|obj| unsafe { Pin::new_unchecked(obj) })
    }

    #[inline]
    pub fn clone_ref(&self, handle: sv_call::Handle) -> Result<sv_call::Handle> {
        let old_ptr = self.decode(handle)?;
        let new = unsafe { old_ptr.as_ref() }.try_clone()?;
        self.insert_ref(new)
    }

    #[inline]
    pub fn insert_ref(&self, value: Ref) -> Result<sv_call::Handle> {
        // SAFETY: The safety condition is guaranteed by the caller.
        let link = PREEMPT.scope(|| self.list.lock().insert(value))?;
        self.encode(link)
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
        let link = self.decode(handle)?;
        PREEMPT.scope(|| self.list.lock().remove(link))
    }

    pub fn remove<T: Send + Sync + Any>(&self, handle: sv_call::Handle) -> Result<Ref<T>> {
        self.decode(handle).and_then(|value| {
            // SAFETY: Dereference within the available range.
            let ptr = unsafe { value.as_ref() };
            if ptr.is::<T>() {
                self.remove_ref(handle).map(|obj| obj.downcast().unwrap())
            } else {
                Err(sv_call::ETYPE)
            }
        })
    }

    pub fn send(&self, handles: &[sv_call::Handle], src: &Channel) -> Result<List> {
        if handles.is_empty() {
            return Ok(List::new());
        }
        PREEMPT.scope(|| {
            { self.list.lock() }.split(handles.iter().map(|&handle| self.decode(handle)), |value| {
                match value.downcast_ref::<Channel>() {
                    Ok(chan) if chan.peer_eq(src) => Err(sv_call::EPERM),
                    Err(_) if !value.features().contains(Feature::SEND) => Err(sv_call::EPERM),
                    _ => Ok(()),
                }
            })
        })
    }

    #[inline]
    pub fn receive(&self, other: &mut List, handles: &mut [sv_call::Handle]) {
        PREEMPT.scope(|| {
            let mut list = self.list.lock();
            for (hdl, obj) in handles.iter_mut().zip(list.merge(other)) {
                *hdl = self.encode(NonNull::from(obj)).unwrap();
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

#[inline]
pub(super) fn init() {
    Azy::force(&node::HR_ARENA);
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
        let old = unsafe { hdl_ptr.r#in().read() }?;
        old.check_null()?;
        let mut obj = SCHED.with_current(|cur| cur.space().handles().remove_ref(old))?;
        let ret = obj.set_features(feat);
        let new = SCHED.with_current(|cur| cur.space().handles().insert_ref(obj))?;
        unsafe { hdl_ptr.out().write(new) }?;
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
