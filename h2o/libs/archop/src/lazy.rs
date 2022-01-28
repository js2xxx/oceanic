use core::{
    cell::{Cell, UnsafeCell},
    hint,
    mem::{self, MaybeUninit},
    ops::Deref,
    ptr,
    sync::atomic::{AtomicUsize, Ordering::*},
};

const UNINIT: usize = 0;
const PROGRESS: usize = 1;
const INIT: usize = 2;
const PANICKED: usize = 3;

pub struct Azy<T, F = fn() -> T> {
    data: UnsafeCell<MaybeUninit<T>>,
    state: AtomicUsize,
    func: Cell<Option<F>>,
}

unsafe impl<T: Send + Sync, F: Send + Sync> Send for Azy<T, F> {}
unsafe impl<T: Send + Sync, F: Send + Sync> Sync for Azy<T, F> {}

impl<T, F> Azy<T, F> {
    #[inline]
    pub const fn new(func: F) -> Self {
        Azy {
            data: UnsafeCell::new(MaybeUninit::uninit()),
            state: AtomicUsize::new(UNINIT),
            func: Cell::new(Some(func)),
        }
    }

    #[track_caller]
    pub fn force(this: &Self) -> &T
    where
        F: FnOnce() -> T,
    {
        loop {
            let state = this.state.load(Acquire);
            match state {
                UNINIT => match this
                    .state
                    .compare_exchange(UNINIT, PROGRESS, Acquire, Acquire)
                {
                    Ok(_) => {
                        struct Guard<'a>(&'a AtomicUsize);
                        impl<'a> Drop for Guard<'a> {
                            #[inline]
                            fn drop(&mut self) {
                                self.0.store(PANICKED, SeqCst);
                            }
                        }

                        let guard = Guard(&this.state);

                        let func = this.func.take().expect("The instance has been poisoned");
                        unsafe { (*this.data.get()).write(func()) };

                        mem::forget(guard);

                        this.state.store(INIT, Release);
                        break unsafe { (*this.data.get()).assume_init_ref() };
                    }
                    Err(_) => hint::spin_loop(),
                },
                PROGRESS => hint::spin_loop(),
                INIT => break unsafe { (*this.data.get()).assume_init_ref() },
                PANICKED => panic!("The initialization function panicked"),
                _ => unreachable!("The instance has been poisoned"),
            }
        }
    }

    #[inline]
    pub fn as_ptr(this: &Self) -> *mut T {
        this.data.get() as *mut T
    }
}

impl<T, F: FnOnce() -> T> Deref for Azy<T, F> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        Self::force(self)
    }
}

impl<T, F> Drop for Azy<T, F> {
    #[inline]
    fn drop(&mut self) {
        if *self.state.get_mut() == INIT {
            unsafe { ptr::drop_in_place((*self.data.get()).as_mut_ptr()) }
        }
    }
}
