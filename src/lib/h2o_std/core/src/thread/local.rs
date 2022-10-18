use core::cell::{Cell, RefCell};

pub struct LocalKey<T: 'static> {
    inner: unsafe fn(Option<&mut Option<T>>) -> Option<&'static T>,
}

#[macro_export]
#[allow_internal_unstable(thread_local)]
macro_rules! thread_local {
    (@KEY, $type:ty, const $init:expr) => {
        {
            unsafe fn get_key(_: Option<&mut Option<$type>>) -> Option<&'static $type> {
                #[thread_local]
                static mut STATE: u8 = 0;

                #[thread_local]
                static mut VAL: $type = $init;

                if !core::mem::needs_drop::<$type>() {
                    return Some(&VAL);
                }

                unsafe extern "C" fn destroy(ptr: *mut u8) {
                    let ptr = ptr.cast::<$type>();
                    ptr.drop_in_place();
                }
                unsafe {
                    match STATE {
                        0 => {
                            $crate::thread::local::register_dtor(core::ptr::addr_of_mut!(VAL).cast(), destroy as _);
                            STATE = 1;
                            Some(&VAL)
                        }
                        1 => Some(&VAL),
                        _ => None,
                    }
                }
            }
            unsafe { $crate::thread::local::LocalKey::new(get_key) }
        }
    };

    (@KEY, $type:ty, $init:expr) => {
        {
            #[inline]
            fn __init() -> $type { $init }

            unsafe fn get_key(init: Option<&mut Option<$type>>) -> Option<&'static $type> {
                #[thread_local]
                static __KEY: $crate::thread::local::fast::Key<$type> =
                    $crate::thread::local::fast::Key::new();

                unsafe {
                    __KEY.get(move || {
                        if let Some(init) = init {
                            if let Some(value) = init.take() {
                                return value;
                            } else if cfg!(debug_assertions) {
                                unreachable!("missing default value");
                            }
                        }
                        __init()
                    })
                }
            }
            unsafe { $crate::thread::local::LocalKey::new(get_key) }
        }
    };

    (@INNER $(#[$attr:meta])* $vis:vis static $name:ident: $type:ty = $($init:tt)*) => {
        $(#[$attr])* $vis const $name: $crate::thread::local::LocalKey<$type> =
            $crate::thread_local!(@KEY, $type, $($init)*);
    };

    {$($(#[$attr:meta])* $vis:vis static $name:ident: $type:ty = const { $init:expr });* $(;)?} => {
        $($crate::thread_local!(@INNER $(#[$attr])* $vis static $name: $type = const $init);)+
    };

    {$($(#[$attr:meta])* $vis:vis static $name:ident: $type:ty = $init:expr);* $(;)?} => {
        $($crate::thread_local!(@INNER $(#[$attr])* $vis static $name: $type = $init);)+
    };
}

impl<T> LocalKey<T> {
    #[doc(hidden)]
    pub const unsafe fn new(
        inner: unsafe fn(Option<&mut Option<T>>) -> Option<&'static T>,
    ) -> Self {
        LocalKey { inner }
    }
}

impl<T: 'static> LocalKey<T> {
    pub fn with<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        self.try_with(f)
            .expect("The thread local value has been destroyed")
    }

    pub fn try_with<F, R>(&'static self, f: F) -> Option<R>
    where
        F: FnOnce(&T) -> R,
    {
        unsafe { Some(f((self.inner)(None)?)) }
    }
}

impl<T: 'static> LocalKey<Cell<T>> {
    #[inline]
    pub fn set(&'static self, value: T) {
        self.with(|c| c.set(value))
    }

    #[inline]
    pub fn get(&'static self) -> T
    where
        T: Copy,
    {
        self.with(|c| c.get())
    }

    #[inline]
    pub fn take(&'static self) -> T
    where
        T: Default,
    {
        self.with(|c| c.take())
    }

    #[inline]
    pub fn replace(&'static self, value: T) -> T {
        self.with(|c| c.replace(value))
    }
}

impl<T: 'static> LocalKey<RefCell<T>> {
    #[inline]
    pub fn with_borrow<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        self.with(|cell| f(&cell.borrow()))
    }

    #[inline]
    pub fn with_borrow_mut<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        self.with(|cell| f(&mut cell.borrow_mut()))
    }

    #[inline]
    pub fn set(&'static self, value: T) {
        self.with(|c| *c.borrow_mut() = value)
    }

    #[inline]
    pub fn take(&'static self) -> T
    where
        T: Default,
    {
        self.with(|cell| cell.take())
    }

    #[inline]
    pub fn replace(&'static self, value: T) -> T {
        self.with(|c| c.replace(value))
    }
}

#[doc(hidden)]
pub mod fast {
    use core::{cell::Cell, fmt, mem};

    use super::{lazy::LazyKeyInner, register_dtor};

    #[derive(Copy, Clone)]
    enum DtorState {
        Unregistered,
        Registered,
        RunningOrHasRun,
    }

    // This data structure has been carefully constructed so that the fast path
    // only contains one branch on x86. That optimization is necessary to avoid
    // duplicated tls lookups on OSX.
    //
    // LLVM issue: https://bugs.llvm.org/show_bug.cgi?id=41722
    pub struct Key<T> {
        // If `LazyKeyInner::get` returns `None`, that indicates either:
        //   * The value has never been initialized
        //   * The value is being recursively initialized
        //   * The value has already been destroyed or is being destroyed
        // To determine which kind of `None`, check `dtor_state`.
        //
        // This is very optimizer friendly for the fast path - initialized but
        // not yet dropped.
        inner: LazyKeyInner<T>,

        // Metadata to keep track of the state of the destructor. Remember that
        // this variable is thread-local, not global.
        dtor_state: Cell<DtorState>,
    }

    impl<T> fmt::Debug for Key<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Key").finish_non_exhaustive()
        }
    }

    impl<T> Key<T> {
        pub const fn new() -> Key<T> {
            Key {
                inner: LazyKeyInner::new(),
                dtor_state: Cell::new(DtorState::Unregistered),
            }
        }

        // note that this is just a publicly-callable function only for the
        // const-initialized form of thread locals, basically a way to call the
        // free `register_dtor` function defined elsewhere in libstd.
        pub unsafe fn register_dtor(a: *mut u8, dtor: unsafe extern "C" fn(*mut u8)) {
            unsafe {
                register_dtor(a, dtor as _);
            }
        }

        pub unsafe fn get<F: FnOnce() -> T>(&self, init: F) -> Option<&'static T> {
            // SAFETY: See the definitions of `LazyKeyInner::get` and
            // `try_initialize` for more information.
            //
            // The caller must ensure no mutable references are ever active to
            // the inner cell or the inner T when this is called.
            // The `try_initialize` is dependant on the passed `init` function
            // for this.
            unsafe {
                match self.inner.get() {
                    Some(val) => Some(val),
                    None => self.try_initialize(init),
                }
            }
        }

        // `try_initialize` is only called once per fast thread local variable,
        // except in corner cases where thread_local dtors reference other
        // thread_local's, or it is being recursively initialized.
        //
        // Macos: Inlining this function can cause two `tlv_get_addr` calls to
        // be performed for every call to `Key::get`.
        // LLVM issue: https://bugs.llvm.org/show_bug.cgi?id=41722
        #[inline(never)]
        unsafe fn try_initialize<F: FnOnce() -> T>(&self, init: F) -> Option<&'static T> {
            // SAFETY: See comment above (this function doc).
            if !mem::needs_drop::<T>() || unsafe { self.try_register_dtor() } {
                // SAFETY: See comment above (this function doc).
                Some(unsafe { self.inner.initialize(init) })
            } else {
                None
            }
        }

        // `try_register_dtor` is only called once per fast thread local
        // variable, except in corner cases where thread_local dtors reference
        // other thread_local's, or it is being recursively initialized.
        unsafe fn try_register_dtor(&self) -> bool {
            match self.dtor_state.get() {
                DtorState::Unregistered => {
                    // SAFETY: dtor registration happens before initialization.
                    // Passing `self` as a pointer while using `destroy_value<T>`
                    // is safe because the function will build a pointer to a
                    // Key<T>, which is the type of self and so find the correct
                    // size.
                    unsafe { Self::register_dtor(self as *const _ as *mut u8, destroy_value::<T>) };
                    self.dtor_state.set(DtorState::Registered);
                    true
                }
                DtorState::Registered => {
                    // recursively initialized
                    true
                }
                DtorState::RunningOrHasRun => false,
            }
        }
    }

    unsafe extern "C" fn destroy_value<T>(ptr: *mut u8) {
        let ptr = ptr as *mut Key<T>;

        // SAFETY:
        //
        // The pointer `ptr` has been built just above and comes from
        // `try_register_dtor` where it is originally a Key<T> coming from `self`,
        // making it non-NUL and of the correct type.
        //
        // Right before we run the user destructor be sure to set the
        // `Option<T>` to `None`, and `dtor_state` to `RunningOrHasRun`. This
        // causes future calls to `get` to run `try_initialize_drop` again,
        // which will now fail, and return `None`.
        unsafe {
            let value = (*ptr).inner.take();
            (*ptr).dtor_state.set(DtorState::RunningOrHasRun);
            drop(value);
        }
    }
}

mod lazy {
    use core::{cell::UnsafeCell, hint, mem};

    pub struct LazyKeyInner<T> {
        inner: UnsafeCell<Option<T>>,
    }

    impl<T> LazyKeyInner<T> {
        pub const fn new() -> LazyKeyInner<T> {
            LazyKeyInner {
                inner: UnsafeCell::new(None),
            }
        }

        pub unsafe fn get(&self) -> Option<&'static T> {
            // SAFETY: The caller must ensure no reference is ever handed out to
            // the inner cell nor mutable reference to the Option<T> inside said
            // cell. This make it safe to hand a reference, though the lifetime
            // of 'static is itself unsafe, making the get method unsafe.
            unsafe { (*self.inner.get()).as_ref() }
        }

        /// The caller must ensure that no reference is active: this method
        /// needs unique access.
        pub unsafe fn initialize<F: FnOnce() -> T>(&self, init: F) -> &'static T {
            // Execute the initialization up front, *then* move it into our slot,
            // just in case initialization fails.
            let value = init();
            let ptr = self.inner.get();

            // SAFETY:
            //
            // note that this can in theory just be `*ptr = Some(value)`, but due to
            // the compiler will currently codegen that pattern with something like:
            //
            //      ptr::drop_in_place(ptr)
            //      ptr::write(ptr, Some(value))
            //
            // Due to this pattern it's possible for the destructor of the value in
            // `ptr` (e.g., if this is being recursively initialized) to re-access
            // TLS, in which case there will be a `&` and `&mut` pointer to the same
            // value (an aliasing violation). To avoid setting the "I'm running a
            // destructor" flag we just use `mem::replace` which should sequence the
            // operations a little differently and make this safe to call.
            //
            // The precondition also ensures that we are the only one accessing
            // `self` at the moment so replacing is fine.
            unsafe {
                let _ = mem::replace(&mut *ptr, Some(value));
            }

            // SAFETY: With the call to `mem::replace` it is guaranteed there is
            // a `Some` behind `ptr`, not a `None` so `unreachable_unchecked`
            // will never be reached.
            unsafe {
                // After storing `Some` we want to get a reference to the contents of
                // what we just stored. While we could use `unwrap` here and it should
                // always work it empirically doesn't seem to always get optimized away,
                // which means that using something like `try_with` can pull in
                // panicking code and cause a large size bloat.
                match *ptr {
                    Some(ref x) => x,
                    None => hint::unreachable_unchecked(),
                }
            }
        }

        /// The other methods hand out references while taking &self.
        /// As such, callers of this method must ensure no `&` and `&mut` are
        /// available and used at the same time.
        #[allow(unused)]
        pub unsafe fn take(&mut self) -> Option<T> {
            // SAFETY: See doc comment for this method.
            unsafe { (*self.inner.get()).take() }
        }
    }
}

#[doc(hidden)]
pub unsafe fn register_dtor(data: *mut u8, dtor: *mut ()) {
    #[link(name = "ldso")]
    extern "C" {
        fn __libc_register_tcb_dtor(data: *mut core::ffi::c_void, dtor: *mut core::ffi::c_void);
    }
    __libc_register_tcb_dtor(data as _, dtor as _)
}
