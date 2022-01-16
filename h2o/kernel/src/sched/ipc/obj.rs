use alloc::{boxed::Box, sync::Arc};
use core::{any::Any, mem};

#[derive(Debug)]
pub struct Object {
    send: bool,
    shared: bool,
    ptr: *mut dyn Any,
}

unsafe impl Send for Object {}
unsafe impl Sync for Object {}

impl Object {
    pub unsafe fn new_unchecked<T: 'static>(o: T, send: bool, shared: bool) -> Object {
        let ptr = if shared {
            Arc::into_raw(Arc::new(o) as Arc<dyn Any>) as *mut dyn Any
        } else {
            Box::into_raw(Box::new(o) as Box<dyn Any>)
        };
        Object { send, shared, ptr }
    }

    #[inline]
    pub fn is_send(&self) -> bool {
        self.send
    }

    #[inline]
    pub unsafe fn deref_unchecked<T: 'static>(&self) -> Option<&T> {
        (&*self.ptr).downcast_ref()
    }

    #[inline]
    pub fn deref<T: Send + 'static>(&self) -> Option<&T> {
        if self.send {
            unsafe { self.deref_unchecked() }
        } else {
            None
        }
    }

    pub fn try_clone(this: &Self) -> Option<Self> {
        if this.send && this.shared {
            let arc = unsafe { Arc::from_raw(this.ptr) };
            let other = Arc::into_raw(Arc::clone(&arc));
            mem::forget(arc);

            Some(Object {
                send: this.send,
                shared: this.shared,
                ptr: other as *mut dyn Any,
            })
        } else {
            None
        }
    }

    pub fn into_inner<T: Send + 'static>(this: Self) -> Result<T, Self> {
        if !this.shared && unsafe { &*this.ptr }.is::<T>() {
            let ret = Box::into_inner(unsafe { Box::from_raw(this.ptr).downcast_unchecked() });
            mem::forget(this);
            Ok(ret)
        } else {
            Err(this)
        }
    }
}

impl Drop for Object {
    fn drop(&mut self) {
        unsafe {
            if self.shared {
                let _ = Arc::from_raw(self.ptr);
            } else {
                let _ = Box::from_raw(self.ptr);
            }
        }
    }
}
