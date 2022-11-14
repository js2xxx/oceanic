use alloc::boxed::Box;
use core::{
    hint,
    num::NonZeroUsize,
    sync::atomic::{AtomicUsize, Ordering::*},
    task::{Poll, Waker},
};

use futures::task::AtomicWaker;
use solvent::prelude::{Dispatcher as Inner, Object, Syscall, ENOENT, ENOSPC};
use solvent_core::sync::{Arsc, CHashMap};

use self::DispError::*;

struct Task {
    pack: Box<dyn PackedSyscall>,
    waker: AtomicWaker,
}

#[derive(Debug)]
pub enum DispError {
    Disconnected,
    TimeOut,
    DidntWait,
    Unpack(solvent::prelude::Error),
    PushRaw(solvent::prelude::Error),
    PopRaw(solvent::prelude::Error),
}

struct Dispatcher {
    id: usize,
    inner: Inner,
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
        static ID: AtomicUsize = AtomicUsize::new(1);
        Dispatcher {
            id: ID.fetch_add(1, SeqCst),
            inner: Inner::new(capacity),
            num_recv: AtomicUsize::new(1),
            tasks: CHashMap::new(),
        }
    }

    #[inline]
    fn disconnected(self: &Arsc<Self>) -> bool {
        self.num_recv.load(SeqCst) == 0 || Arsc::count(self) <= 1
    }

    fn poll_receive(self: &Arsc<Self>) -> Poll<Result<(), DispError>> {
        match self.inner.pop_raw() {
            Ok(res) => {
                let Task { waker, mut pack } = loop {
                    match self.tasks.remove(&res.key) {
                        Some(task) => break task,
                        None => hint::spin_loop(),
                    }
                };
                // We need to inform the task where an internal error occurred.
                let res = pack.unpack(res.result, NonZeroUsize::new(res.signal));
                waker.wake();
                Poll::Ready(res.map_err(Unpack))
            }
            Err(ENOENT) => {
                if self.disconnected() {
                    Poll::Ready(Err(Disconnected))
                } else {
                    Poll::Pending
                }
            }
            Err(err) => Err(PopRaw(err))?,
        }
    }

    fn poll_send<P>(
        self: &Arsc<Self>,
        obj: &impl Object,
        level_triggered: bool,
        signal: usize,
        pack: P,
        waker: &Waker,
    ) -> Result<Result<usize, DispError>, P>
    where
        P: PackedSyscall + 'static,
    {
        if self.disconnected() {
            return Ok(Err(Disconnected));
        }

        let syscall = pack.raw();
        let key = match self
            .inner
            .push_raw(obj, level_triggered, signal, syscall.as_ref())
        {
            Err(ENOSPC) => return Err(pack),
            key => match key {
                Ok(key) => key,
                Err(err) => return Ok(Err(PushRaw(err))),
            },
        };
        let task = Task {
            pack: Box::new(pack),
            waker: AtomicWaker::new(),
        };
        task.waker.register(waker);
        let old = self.tasks.insert(key, task);
        assert!(old.is_none());
        Ok(Ok(key))
    }

    fn update(self: &Arsc<Self>, key: usize, waker: &Waker) -> Result<(), DispError> {
        if self.disconnected() {
            return Err(Disconnected);
        }

        if let Some(task) = self.tasks.get_mut(&key) {
            task.waker.register(waker);
            Ok(())
        } else {
            Err(DidntWait)
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
    ) -> Result<Result<usize, DispError>, P>
    where
        P: PackedSyscall + 'static,
    {
        self.disp
            .poll_send(obj, level_triggered, signal, pack, waker)
    }

    #[inline]
    pub fn update(&self, key: usize, waker: &Waker) -> Result<(), DispError> {
        self.disp.update(key, waker)
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
    pub fn poll_receive(&self) -> Poll<Result<(), DispError>> {
        self.disp.poll_receive()
    }

    #[inline]
    pub fn id(&self) -> usize {
        self.disp.id
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
        if self.disp.num_recv.fetch_sub(1, SeqCst) == 1 {
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

    fn unpack(&mut self, result: usize, signal: Option<NonZeroUsize>) -> solvent::prelude::Result;
}
