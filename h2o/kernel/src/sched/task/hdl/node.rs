use alloc::sync::{Arc, Weak};
use core::{
    any::Any,
    marker::{PhantomPinned, Unsize},
    ops::{CoerceUnsized, Deref},
};

use sv_call::{Feature, Result};

use super::DefaultFeature;
use crate::sched::Event;

pub const MAX_HANDLE_COUNT: usize = 1 << 16;

#[derive(Debug)]
pub struct Ref<T: ?Sized = dyn Any + Send + Sync> {
    _marker: PhantomPinned,
    event: Weak<dyn Event>,
    feat: Feature,
    obj: Arc<T>,
}

unsafe impl<T: ?Sized> Send for Ref<T> {}

impl<T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<Ref<U>> for Ref<T> {}

impl<T: ?Sized> Ref<T> {
    /// # Safety
    ///
    /// The caller must ensure that `T` is [`Send`] if `send` and [`Sync`] if
    /// `sync`.
    pub unsafe fn try_new_unchecked(
        data: T,
        feat: Feature,
        event: Option<Weak<dyn Event>>,
    ) -> sv_call::Result<Self>
    where
        T: Sized,
    {
        Self::from_raw_unchecked(Arc::try_new(data)?, feat, event)
    }

    /// # Safety
    ///
    /// The caller must ensure that `T` is [`Send`] if `send` and [`Sync`] if
    /// `sync`.
    pub unsafe fn from_raw_unchecked(
        obj: Arc<T>,
        feat: Feature,
        event: Option<Weak<dyn Event>>,
    ) -> sv_call::Result<Self> {
        if event.is_none() && feat.contains(Feature::WAIT) {
            return Err(sv_call::EPERM);
        }
        let event = event.unwrap_or(Weak::<crate::sched::BasicEvent>::new() as _);
        Ok(Ref {
            _marker: PhantomPinned,
            event,
            feat,
            obj,
        })
    }

    #[inline]
    pub fn try_new(data: T, event: Option<Weak<dyn Event>>) -> sv_call::Result<Self>
    where
        T: DefaultFeature + Sized,
    {
        unsafe { Self::try_new_unchecked(data, T::default_features(), event) }
    }

    #[inline]
    pub fn from_raw(obj: Arc<T>, event: Option<Weak<dyn Event>>) -> sv_call::Result<Self>
    where
        T: DefaultFeature,
    {
        unsafe { Self::from_raw_unchecked(obj, T::default_features(), event) }
    }

    #[inline]
    pub fn into_raw(this: Self) -> Arc<T> {
        this.obj
    }

    /// # Safety
    ///
    /// The caller must ensure that `self` is owned by the current task if its
    /// not [`Send`].
    pub unsafe fn deref_unchecked(&self) -> &Arc<T> {
        &self.obj
    }

    #[inline]
    pub fn event(&self) -> &Weak<dyn Event> {
        &self.event
    }

    #[inline]
    pub fn features(&self) -> Feature {
        self.feat
    }

    pub fn set_features(&mut self, feat: Feature) -> Result {
        if feat & !self.feat == Feature::empty() {
            self.feat = feat;
            Ok(())
        } else {
            Err(sv_call::EPERM)
        }
    }

    pub fn try_unwrap(this: Self) -> core::result::Result<T, Self>
    where
        T: Sized,
    {
        Arc::try_unwrap(this.obj).map_err(|obj| Ref {
            _marker: PhantomPinned,
            event: this.event,
            feat: this.feat,
            obj,
        })
    }
}

impl<T: ?Sized + Send + Sync> Deref for Ref<T> {
    type Target = Arc<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // SAFETY: It's `Send`.
        unsafe { self.deref_unchecked() }
    }
}

impl Ref {
    #[inline]
    pub fn is<T: Any>(&self) -> bool {
        self.obj.is::<T>()
    }

    pub fn downcast_ref<T: Any>(&self) -> Result<&Ref<T>> {
        if self.is::<T>() {
            Ok(unsafe { &*(self as *const Ref as *const Ref<T>) })
        } else {
            Err(sv_call::ETYPE)
        }
    }

    pub fn downcast<T: Any + Send + Sync>(self) -> core::result::Result<Ref<T>, Self> {
        match self.obj.downcast() {
            Ok(obj) => Ok(Ref {
                _marker: PhantomPinned,
                event: self.event,
                feat: self.feat,
                obj,
            }),
            Err(obj) => Err(Ref {
                _marker: PhantomPinned,
                event: self.event,
                feat: self.feat,
                obj,
            }),
        }
    }

    /// # Safety
    ///
    /// The caller must ensure that every reference to the underlying object is
    /// not to be moved to another task if its not [`Send`] or [`Sync`].
    #[inline]
    #[must_use = "Don't make useless clonings"]
    unsafe fn clone_unchecked(&self) -> Ref {
        Ref {
            _marker: PhantomPinned,
            event: Weak::clone(&self.event),
            feat: self.feat,
            obj: Arc::clone(&self.obj),
        }
    }

    pub fn try_clone(&self) -> Result<Ref> {
        let feat = self.features();
        if feat.contains(Feature::SEND | Feature::SYNC) {
            // SAFETY: The underlying object is `send` and `sync`.
            Ok(unsafe { self.clone_unchecked() })
        } else {
            Err(sv_call::EPERM)
        }
    }
}
