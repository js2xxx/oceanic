use core::{marker::PhantomData, mem, mem::ManuallyDrop, ops::Deref, time::Duration};

pub use sv_call::{Feature, Handle};

use crate::error::Result;

pub trait Object {
    /// # Safety
    ///
    /// The ownership of the object must not be moved if it's still in use.
    unsafe fn raw(&self) -> sv_call::Handle;

    /// # Safety
    ///
    /// The handle must be of the same type as the object and has its own
    /// ownership.
    unsafe fn from_raw(raw: sv_call::Handle) -> Self;

    fn into_raw(this: Self) -> sv_call::Handle
    where
        Self: Sized,
    {
        // SAFETY: We move the ownership and guarantee that the object is not used
        // anymore.
        let raw = unsafe { this.raw() };
        mem::forget(this);
        raw
    }

    fn try_clone(this: &Self) -> Result<Self>
    where
        Self: Sized,
    {
        // SAFETY: We don't move the ownership of the handle.
        let handle = unsafe { sv_call::sv_obj_clone(unsafe { this.raw() }).into_res()? };
        // SAFETY: The handle is freshly allocated.
        Ok(unsafe { Self::from_raw(handle) })
    }

    /// # Safety
    ///
    /// This function must be called only in the drop context and the object
    /// must not be used anymore.
    unsafe fn try_drop(this: &mut Self) -> Result {
        // SAFETY: We move the ownership and guarantee that the object is not used
        // anymore because we're in the drop context.
        sv_call::sv_obj_drop(unsafe { this.raw() }).into_res()
    }

    fn try_wait(&self, timeout: Duration, wake_all: bool, signal: usize) -> Result<usize> {
        unsafe {
            sv_call::sv_obj_wait(
                // SAFETY: We don't move the ownership of the handle.
                unsafe { self.raw() },
                crate::time::try_into_us(timeout)?,
                wake_all,
                signal,
            )
            .into_res()
            .map(|value| value as usize)
        }
    }

    fn try_wait_async(&self, wake_all: bool, signal: usize) -> Result<crate::ipc::Waiter> {
        // SAFETY: We don't move the ownership of the handle.
        let handle =
            unsafe { sv_call::sv_obj_await(unsafe { self.raw() }, wake_all, signal).into_res()? };
        // SAFETY: The handle is freshly allocated.
        Ok(unsafe { Object::from_raw(handle) })
    }

    fn reduce_features(self, features: Feature) -> Result<Self>
    where
        Self: Sized,
    {
        let mut handle = Self::into_raw(self);
        unsafe { sv_call::sv_obj_feat(&mut handle, features) }.into_res()?;
        // SAFETY: The handle is freshly allocated.
        Ok(unsafe { Self::from_raw(handle) })
    }

    fn as_ref(&self) -> Ref<'_, Self>
    where
        Self: Sized,
    {
        // SAFETY: The handle is valid and the ownership is not transferred.
        unsafe { Ref::from_raw(self.raw()) }
    }

    fn leak(self) -> Ref<'static, Self>
    where
        Self: Sized,
    {
        // SAFETY: The handle is valid and the ownership is not transferred.
        unsafe { Ref::from_raw(Self::into_raw(self)) }
    }
}

#[macro_export]
macro_rules! impl_obj {
    ($name:ident) => {
        impl $crate::obj::Object for $name {
            unsafe fn raw(&self) -> sv_call::Handle {
                self.0
            }

            unsafe fn from_raw(raw: sv_call::Handle) -> Self {
                Self(raw)
            }
        }
    };

    (@CLONE, $name:ident) => {
        impl Clone for $name {
            fn clone(&self) -> Self {
                $crate::obj::Object::try_clone(self).expect("Failed to clone object")
            }
        }
    };

    (@DROP, $name:ident) => {
        impl Drop for $name {
            fn drop(&mut self) {
                // SAFETY: We're calling in the drop context.
                unsafe { $crate::obj::Object::try_drop(self) }.expect("Failed to drop object")
            }
        }
    };
}

#[derive(Clone, Copy)]
pub struct Ref<'a, T: ?Sized> {
    marker: PhantomData<&'a T>,
    inner: ManuallyDrop<T>,
}

impl<'a, T: Object> From<&'a T> for Ref<'a, T> {
    fn from(obj: &'a T) -> Self {
        // SAFETY: The handle is valid and the ownership is not transferred.
        unsafe { Ref::from_raw(obj.raw()) }
    }
}

impl<'a, T: Object> Ref<'a, T> {
    /// # Safety
    ///
    /// The handle must be of the same type as the object.u
    pub unsafe fn from_raw(raw: sv_call::Handle) -> Self {
        Ref {
            marker: PhantomData,
            // SAFETY: The ownership of the handle is not transferred.
            inner: ManuallyDrop::new(unsafe { T::from_raw(raw) }),
        }
    }

    pub fn into_raw(this: Self) -> sv_call::Handle {
        T::into_raw(ManuallyDrop::into_inner(this.inner))
    }
}

impl<'a, T: ?Sized> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
