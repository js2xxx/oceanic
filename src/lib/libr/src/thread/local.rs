use core::cell::{Cell, RefCell};

pub struct LocalKey<T: 'static> {
    inner: unsafe fn() -> Option<&'static T>,
}

#[macro_export]
#[allow_internal_unstable(thread_local)]
macro_rules! thread_local {
    (@INNER $(#[$attr:meta])* $vis:vis static $name:ident: $type:ty = $init:expr) => {
        $(#[$attr])* $vis static $name: $crate::thread::local::LocalKey<$type>
            = $crate::thread::local::LocalKey::new(
            {
                unsafe fn get_key() -> Option<&'static $type> {
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

                    #[link(name = "ldso")]
                    extern "C" {
                        fn __libc_register_tcb_dtor(data: *mut core::ffi::c_void, dtor: *mut core::ffi::c_void);
                    }

                    unsafe {
                        match STATE {
                            0 => {
                                __libc_register_tcb_dtor(core::ptr::addr_of_mut!(VAL).cast(), destroy as _);
                                STATE = 1;
                                Some(&VAL)
                            }
                            1 => Some(&VAL),
                            _ => None,
                        }
                    }
                }
                get_key
            }
        );
    };
    {$($(#[$attr:meta])* $vis:vis static $name:ident: $type:ty = $init:expr);* $(;)?} => {
        $(thread_local!(@INNER $(#[$attr])* $vis static $name: $type = $init);)+
    };
}

impl<T> LocalKey<T> {
    pub const fn new(inner: unsafe fn() -> Option<&'static T>) -> Self {
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
        unsafe { Some(f((self.inner)()?)) }
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
