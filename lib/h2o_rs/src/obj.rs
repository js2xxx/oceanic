use core::mem;

pub trait Object {
    /// # Safety
    ///
    /// The ownership of the object must not be moved if it in use.
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

    fn try_clone(this: &Self) -> crate::error::Result<Self>
    where
        Self: Sized,
    {
        // SAFETY: We don't move the ownership of the handle.
        let handle = sv_call::obj_clone(unsafe { this.raw() })?;
        // SAFETY: The handle is freshly allocated.
        Ok(unsafe { Self::from_raw(handle) })
    }

    /// # Safety
    ///
    /// This function must be called only in the drop context and the object
    /// must not be used anymore.
    unsafe fn try_drop(this: &mut Self) -> crate::error::Result {
        // SAFETY: We move the ownership and guarantee that the object is not used
        // anymore because we're in the drop context.
        sv_call::obj_drop(unsafe { this.raw() })
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
