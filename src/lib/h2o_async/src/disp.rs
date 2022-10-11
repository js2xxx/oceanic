use alloc::boxed::Box;
use core::{
    num::NonZeroUsize,
    sync::atomic::{AtomicUsize, Ordering::*},
    task::{Poll, Waker},
};

use futures::task::AtomicWaker;
use solvent::prelude::{
    Dispatcher as Inner, Object, Result, Syscall, ENOENT, ENOSPC, EPIPE, ETIME,
};
use solvent_std::sync::{Arsc, CHashMap};

struct Task {
    pack: Box<dyn PackedSyscall>,
    waker: AtomicWaker,
}

const NORMAL: usize = 0;
const DISCONNECTED: usize = isize::MAX as usize;

struct Dispatcher {
    inner: Inner,
    state: AtomicUsize,
    num_recv: AtomicUsize,
    tasks: CHashMap<usize, Task>,
}

// SAFETY: Usually, `pack` field in `Task` should be `Sync` in order to derive
// `Sync` for this structure. However, we here guarantee that `pack` don't
// expose its reference to any context, meaning that it don't need to be `Sync`.
unsafe impl Sync for Dispatcher {}

impl Dispatcher {
    #[inline]
    fn new(capacity: usize) -> Self {
        Dispatcher {
            inner: Inner::new(capacity),
            state: AtomicUsize::new(NORMAL),
            num_recv: AtomicUsize::new(1),
            tasks: CHashMap::new(),
        }
    }

    fn poll_receive(&self) -> Poll<Result> {
        match self.inner.pop_raw() {
            Ok(res) => {
                let Task { waker, mut pack } = self.tasks.remove(&res.key).ok_or(ETIME)?;
                // We need to inform the task where an internal error occurred.
                let res = pack.unpack(res.result, NonZeroUsize::new(res.signal));
                waker.wake();
                Poll::Ready(res)
            }
            Err(ENOENT) => {
                if self.state.load(SeqCst) == DISCONNECTED {
                    Poll::Ready(Err(EPIPE))
                } else {
                    Poll::Pending
                }
            }
            Err(err) => Err(err)?,
        }
    }

    fn poll_send<P>(
        &self,
        obj: &impl Object,
        level_triggered: bool,
        signal: usize,
        pack: P,
        waker: &Waker,
    ) -> core::result::Result<Result<usize>, P>
    where
        P: PackedSyscall + 'static,
    {
        if self.state.load(SeqCst) == DISCONNECTED {
            return Ok(Err(EPIPE));
        }

        let syscall = pack.raw();
        let key = match self
            .inner
            .push_raw(obj, level_triggered, signal, syscall.as_ref())
        {
            Err(ENOSPC) => return Err(pack),
            key => match key {
                Ok(key) => key,
                Err(err) => return Ok(Err(err)),
            },
        };
        let task = Task {
            pack: Box::new(pack),
            waker: AtomicWaker::new(),
        };
        task.waker.register(waker);
        self.tasks.insert(key, task);
        Ok(Ok(key))
    }

    fn update(&self, key: usize, waker: &Waker) -> Result {
        if self.state.load(SeqCst) == DISCONNECTED {
            return Err(EPIPE);
        }

        if let Some(task) = self.tasks.get_mut(&key) {
            task.waker.register(waker);
            Ok(())
        } else {
            Err(ENOENT)
        }
    }
}

#[derive(Clone)]
pub struct DispSender {
    disp: Arsc<Dispatcher>,
}

impl DispSender {
    #[inline]
    fn new(disp: Arsc<Dispatcher>) -> Self {
        DispSender { disp }
    }

    /// # Returns
    ///
    /// The outer result indicates whether the dispatcher queue is full, with
    /// the same meaning as [`Poll`], while the inner one indicates whether some
    /// actual error occurred, and should (not) be immediately passed to the
    /// outer context.
    #[inline]
    pub fn poll_send<P>(
        &self,
        obj: &impl Object,
        level_triggered: bool,
        signal: usize,
        pack: P,
        waker: &Waker,
    ) -> core::result::Result<Result<usize>, P>
    where
        P: PackedSyscall + 'static,
    {
        self.disp
            .poll_send(obj, level_triggered, signal, pack, waker)
    }

    #[inline]
    pub fn update(&self, key: usize, waker: &Waker) -> Result {
        self.disp.update(key, waker)
    }
}

impl Drop for DispSender {
    #[inline]
    fn drop(&mut self) {
        self.disp.state.store(DISCONNECTED, SeqCst);
    }
}

pub struct DispReceiver {
    disp: Arsc<Dispatcher>,
}

impl DispReceiver {
    #[inline]
    fn new(disp: Arsc<Dispatcher>) -> Self {
        DispReceiver { disp }
    }

    #[inline]
    pub fn poll_receive(&self) -> Poll<Result> {
        self.disp.poll_receive()
    }
}

impl Clone for DispReceiver {
    fn clone(&self) -> Self {
        let disp = Arsc::clone(&self.disp);
        disp.num_recv.fetch_add(1, SeqCst);
        DispReceiver { disp }
    }
}

impl Drop for DispReceiver {
    fn drop(&mut self) {
        self.disp.state.store(DISCONNECTED, SeqCst);
        if self.disp.num_recv.fetch_sub(1, SeqCst) == 0 {
            let tasks = self.disp.tasks.take();
            for (_, task) in tasks {
                let Task { mut pack, waker } = task;
                let _ = pack.unpack(0, None);
                waker.wake();
            }
        }
    }
}

#[inline]
pub fn dispatch(capacity: usize) -> (DispSender, DispReceiver) {
    let inner = Arsc::new(Dispatcher::new(capacity));
    (
        DispSender::new(Arsc::clone(&inner)),
        DispReceiver::new(inner),
    )
}

/// # Safety
///
/// The implementation must not expose its reference to the outer context.
pub unsafe trait PackedSyscall: Send {
    fn raw(&self) -> Option<Syscall>;

    fn unpack(&mut self, result: usize, signal: Option<NonZeroUsize>) -> Result;
}
