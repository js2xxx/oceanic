use alloc::alloc::Global;
use core::{
    alloc::{AllocError, Allocator, Layout},
    any::Any,
    fmt,
    marker::{PhantomData, Unsize},
    mem::{self, ManuallyDrop, MaybeUninit},
    ops::{CoerceUnsized, Deref, Receiver},
    ptr::{self, NonNull},
    sync::atomic::{self, AtomicUsize, Ordering::*},
};

const REF_COUNT_MAX: usize = isize::MAX as usize;
#[cfg(target_pointer_width = "64")]
const REF_COUNT_SATURATED: usize = 0xC000_0000_0000_0000;
#[cfg(target_pointer_width = "32")]
const REF_COUNT_SATURATED: usize = 0xC000_0000;

pub struct Arsc<T: ?Sized, A: Allocator = Global> {
    inner: NonNull<ArscInner<T, A>>,
    _marker: PhantomData<ArscInner<T, A>>,
}

struct ArscInner<T: ?Sized, A: Allocator> {
    ref_count: AtomicUsize,
    alloc: ManuallyDrop<A>,
    data: T,
}

impl<T: ?Sized, A: Allocator> Receiver for Arsc<T, A> {}

impl<T: ?Sized + Unsize<U>, U: ?Sized, A: Allocator> CoerceUnsized<Arsc<U, A>> for Arsc<T, A> {}

unsafe impl<T: ?Sized + Send + Sync, A: Allocator + Send + Sync> Send for Arsc<T, A> {}

unsafe impl<T: ?Sized + Send + Sync, A: Allocator + Send + Sync> Sync for Arsc<T, A> {}

impl<T, A: Allocator> Arsc<T, A> {
    pub fn try_new_in(data: T, alloc: A) -> Result<Self, AllocError> {
        let mem = alloc.allocate(Layout::new::<ArscInner<T, A>>())?;
        let inner = mem.cast::<ArscInner<T, A>>();
        // SAFETY: Move the data and the allocator into uninitialized memory.
        unsafe {
            inner.as_ptr().write(ArscInner {
                ref_count: AtomicUsize::new(1),
                alloc: ManuallyDrop::new(alloc),
                data,
            })
        };
        Ok(Arsc {
            inner,
            _marker: PhantomData,
        })
    }

    pub fn try_new_uninit_in(alloc: A) -> Result<Arsc<MaybeUninit<T>, A>, AllocError> {
        let mem = alloc.allocate(Layout::new::<ArscInner<T, A>>())?;
        let inner = mem.cast::<ArscInner<MaybeUninit<T>, A>>();
        unsafe {
            inner.as_ptr().write(ArscInner {
                ref_count: AtomicUsize::new(1),
                alloc: ManuallyDrop::new(alloc),
                data: MaybeUninit::uninit(),
            })
        };
        Ok(Arsc {
            inner,
            _marker: PhantomData,
        })
    }
}

impl<T> Arsc<T, Global> {
    #[inline]
    pub fn try_new(data: T) -> Result<Self, AllocError> {
        Self::try_new_in(data, Global)
    }

    #[inline]
    pub fn try_new_uninit() -> Result<Arsc<MaybeUninit<T>, Global>, AllocError> {
        Self::try_new_uninit_in(Global)
    }
}

impl<T, A: Allocator> Arsc<MaybeUninit<T>, A> {
    /// # Safety
    ///
    /// The caller must ensure a valid value of `T` stored in the `Arsc`.
    pub unsafe fn assume_init(this: Self) -> Arsc<T, A> {
        unsafe { Arsc::from_inner(ManuallyDrop::new(this).inner.cast()) }
    }
}

impl<T: ?Sized, A: Allocator> Arsc<T, A> {
    /// # Safety
    ///
    /// The caller must ensure the validity of the pointer and the reference
    /// count.
    #[inline]
    unsafe fn from_inner(inner: NonNull<ArscInner<T, A>>) -> Self {
        Arsc {
            inner,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        a.inner.as_ptr() == b.inner.as_ptr()
    }

    #[inline]
    pub fn as_ptr(this: &Self) -> *const T {
        &**this
    }

    /// # Safety
    ///
    /// The caller must ensure the exclusiveness of the mutable reference.
    pub unsafe fn get_mut_unchecked(this: &mut Self) -> &mut T {
        unsafe { &mut this.inner.as_mut().data }
    }

    pub fn get_mut(this: &mut Self) -> Option<&mut T> {
        let ref_count = unsafe { &this.inner.as_ref().ref_count };
        if ref_count.compare_exchange(1, 0, Acquire, Relaxed).is_err() {
            None
        } else {
            ref_count.store(1, Release);
            // SAFETY: We own the unique reference.
            Some(unsafe { Self::get_mut_unchecked(this) })
        }
    }
}

impl<T, A: Allocator> Arsc<T, A> {
    pub fn try_make_mut_with<F, E>(this: &mut Self, clone: F) -> Result<&mut T, E>
    where
        E: From<AllocError>,
        F: FnOnce(&T) -> Result<(T, A), E>,
    {
        // SAFETY: Allowed immutable reference.
        let ref_count = unsafe { &this.inner.as_ref().ref_count };
        if ref_count.compare_exchange(1, 0, Acquire, Relaxed).is_err() {
            let (data, alloc) = clone(this)?;
            *this = Self::try_new_in(data, alloc)?;
        } else {
            ref_count.store(1, Release);
        }
        // SAFETY: We own the unique reference.
        Ok(unsafe { Self::get_mut_unchecked(this) })
    }

    #[inline]
    pub fn try_make_mut(this: &mut Self) -> Result<&mut T, AllocError>
    where
        T: Clone,
        A: Clone,
    {
        // SAFETY: Allowed immutable reference.
        let alloc = unsafe { &this.inner.as_ref().alloc };
        Self::try_make_mut_with(this, |t| Ok((T::clone(t), A::clone(alloc))))
    }
}

impl<A: Allocator> Arsc<dyn Any, A> {
    pub fn downcast<T: Any>(self) -> core::result::Result<Arsc<T, A>, Self> {
        if (*self).is::<T>() {
            unsafe {
                let inner = self.inner.cast::<ArscInner<T, A>>();
                mem::forget(self);
                Ok(Arsc::from_inner(inner))
            }
        } else {
            Err(self)
        }
    }
}

impl<T: ?Sized, A: Allocator> Deref for Arsc<T, A> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // SAFETY: Allowed immutable reference.
        unsafe { &self.inner.as_ref().data }
    }
}

impl<T: ?Sized, A: Allocator> Clone for Arsc<T, A> {
    fn clone(&self) -> Self {
        // SAFETY: Allowed immutable reference.
        let inner = unsafe { self.inner.as_ref() };
        let count = inner.ref_count.fetch_add(1, Relaxed);

        if count >= REF_COUNT_MAX {
            inner.ref_count.store(REF_COUNT_SATURATED, Relaxed);
            log::warn!(
                "Reference count overflow detected at {:?}. Leaking memory!",
                self as *const _
            );
        }

        // SAFETY: We have just incremented the reference count.
        unsafe { Self::from_inner(self.inner) }
    }
}

impl<T: ?Sized + fmt::Debug, A: Allocator> fmt::Debug for Arsc<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Arsc").field(&self.deref()).finish()
    }
}

impl<T: ?Sized, A: Allocator> Drop for Arsc<T, A> {
    fn drop(&mut self) {
        // SAFETY: Allowed immutable reference.
        let inner = unsafe { self.inner.as_ref() };

        let count = inner.ref_count.fetch_sub(1, Release);

        if count >= REF_COUNT_MAX {
            inner.ref_count.store(REF_COUNT_SATURATED, Relaxed);
            log::warn!(
                "Reference count overflow detected at {:?}. Leaking memory!",
                self as *const _
            );
        } else if count == 1 {
            atomic::fence(Acquire);

            // SAFETY: No more references are available and the only `alloc` instance is
            // being moved out.
            let alloc = unsafe { ManuallyDrop::take(&mut self.inner.as_mut().alloc) };
            // SAFETY: `alloc` field won't be dropped in place.
            unsafe { ptr::drop_in_place(self.inner.as_ptr()) };
            // SAFETY: No more dereferencing to `self.inner`.
            unsafe {
                alloc.deallocate(
                    self.inner.cast(),
                    Layout::for_value_raw(self.inner.as_ptr()),
                )
            };
        }
    }
}

impl<T: ?Sized + PartialEq, A: Allocator> PartialEq for Arsc<T, A> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}

impl<T: ?Sized + Eq, A: Allocator> Eq for Arsc<T, A> {}

impl<T: ?Sized + PartialOrd, A: Allocator> PartialOrd for Arsc<T, A> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.deref().partial_cmp(other)
    }
}

impl<T: ?Sized + Ord, A: Allocator> Ord for Arsc<T, A> {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.deref().cmp(other)
    }
}
