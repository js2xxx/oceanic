pub mod imp;

use alloc::{boxed::Box, fmt, string::String};
use core::{
    marker::PhantomData,
    mem,
    num::NonZeroU64,
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering::Relaxed},
    time::Duration,
};

use solvent::error::Result;

use crate::sync::{imp::Parker, Arsc, Mutex};

#[derive(Debug)]
pub struct Builder {
    stack: usize,
    name: Option<String>,
}

impl Default for Builder {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    #[inline]
    pub const fn new() -> Self {
        Builder {
            stack: 0,
            name: None,
        }
    }

    #[inline]
    pub fn name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    #[inline]
    pub fn stack(mut self, stack: usize) -> Self {
        self.stack = stack;
        self
    }

    pub fn spawn<F, T>(self, f: F) -> Result<JoinHandle<T>>
    where
        F: FnOnce() -> T,
        F: Send + 'static,
        T: Send + 'static,
    {
        unsafe { self.spawn_unchecked(f) }
    }

    /// # Safety
    ///
    /// The caller has to ensure that the spawned thread does not outlive any
    /// references in the supplied thread closure and its return type.
    pub unsafe fn spawn_unchecked<'a, F, T>(self, f: F) -> Result<JoinHandle<T>>
    where
        F: FnOnce() -> T,
        F: Send + 'a,
        T: Send + 'a,
    {
        let inner = unsafe { self.spawn_inner(f) }?;
        Ok(JoinHandle(inner))
    }

    unsafe fn spawn_inner<'a, 'scope, F, T>(self, f: F) -> Result<JoinInner<'scope, T>>
    where
        F: FnOnce() -> T + 'a,
        T: 'a,
        'scope: 'a,
    {
        let thread = Thread::new(self.name);
        let t2 = thread.clone();

        let packet = Arsc::new(Mutex::new(None::<T>));
        let p2 = packet.clone();

        let main = move || {
            let _ = t2;
            *p2.lock() = Some(f());
        };

        let native = imp::Thread::new(thread.inner.name.as_deref(), self.stack, unsafe {
            mem::transmute::<Box<dyn FnOnce() + 'a>, Box<dyn FnOnce() + 'static>>(Box::new(main))
        })?;
        Ok(JoinInner {
            native,
            thread,
            packet,
            _marker: PhantomData,
        })
    }
}

pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T,
    F: Send + 'static,
    T: Send + 'static,
{
    Builder::new().spawn(f).expect("failed to spawn thread")
}

#[inline]
pub fn sleep(duration: Duration) {
    imp::Thread::sleep(duration)
}

#[inline]
pub fn yield_now() {
    imp::Thread::yield_now()
}

struct JoinInner<'a, T> {
    native: imp::Thread,
    thread: Thread,
    packet: Arsc<Mutex<Option<T>>>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T> JoinInner<'a, T> {
    fn join(mut self) -> T {
        self.native.join();
        Arsc::get_mut(&mut self.packet)
            .unwrap()
            .get_mut()
            .take()
            .unwrap()
    }
}

pub struct JoinHandle<T: 'static>(JoinInner<'static, T>);

unsafe impl<T> Send for JoinHandle<T> {}
unsafe impl<T> Sync for JoinHandle<T> {}

impl<T> JoinHandle<T> {
    #[inline]
    #[must_use]
    pub fn thread(&self) -> &Thread {
        &self.0.thread
    }

    #[inline]
    pub fn join(self) -> T {
        self.0.join()
    }

    #[inline]
    pub fn is_finished(&self) -> bool {
        Arsc::count(&self.0.packet) == 1
    }
}

struct Inner {
    name: Option<String>,
    id: NonZeroU64,
    parker: Parker,
}

impl Inner {
    fn parker(self: Pin<&Self>) -> Pin<&Parker> {
        unsafe { self.map_unchecked(|s| &s.parker) }
    }
}

#[derive(Clone)]
pub struct Thread {
    inner: Pin<Arsc<Inner>>,
}

impl Thread {
    fn next_id() -> NonZeroU64 {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        NonZeroU64::new(COUNTER.fetch_add(1, Relaxed)).unwrap()
    }
    pub(crate) fn new(name: Option<String>) -> Thread {
        Thread {
            inner: Arsc::pin(Inner {
                name,
                id: Self::next_id(),
                parker: Parker::new(),
            }),
        }
    }

    #[inline]
    #[must_use]
    pub fn id(&self) -> u64 {
        self.inner.id.get()
    }

    pub fn unpark(&self) {
        self.inner.as_ref().parker().unpark()
    }

    #[inline]
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.inner.name.as_deref()
    }
}

impl fmt::Debug for Thread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Thread")
            .field("id", &self.id())
            .field("name", &self.name())
            .finish_non_exhaustive()
    }
}

fn _assert_sync_and_send() {
    fn _assert_both<T: Send + Sync>() {}
    _assert_both::<JoinHandle<()>>();
    _assert_both::<Thread>();
}

mod current {

}