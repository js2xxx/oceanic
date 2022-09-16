use alloc::{boxed::Box, collections::BTreeMap};
use core::task::{Poll, Waker};

use futures::task::AtomicWaker;
use solvent::prelude::{Dispatcher as Inner, Object, Result, Syscall, ENOENT, ETIME};
use solvent_std::sync::Mutex;

struct Task {
    pack: Box<dyn PackedSyscall>,
    waker: AtomicWaker,
}

pub struct Dispatcher {
    inner: Inner,
    tasks: Mutex<BTreeMap<usize, Task>>,
}

impl Dispatcher {
    #[inline]
    pub fn new(capacity: usize) -> Self {
        Dispatcher {
            inner: Inner::new(capacity),
            tasks: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn poll_next(&self) -> Poll<Result> {
        match self.inner.pop_raw() {
            Ok(res) => {
                let Task { waker, mut pack } = self.tasks.lock().remove(&res.key).ok_or(ETIME)?;
                pack.unpack(res.result, res.canceled)?;
                waker.wake();
                Poll::Ready(Ok(()))
            }
            Err(ENOENT) => Poll::Pending,
            Err(err) => Err(err)?,
        }
    }

    fn push_inner<K>(&self, key: K, pack: Box<dyn PackedSyscall>, waker: &Waker) -> Result
    where
        K: FnOnce(&Syscall) -> Result<usize>,
    {
        let syscall = pack.raw();
        let key = key(&syscall)?;
        let task = Task {
            pack,
            waker: AtomicWaker::new(),
        };
        task.waker.register(waker);
        self.tasks.lock().insert(key, task);
        Ok(())
    }

    #[inline]
    pub fn push(
        &self,
        obj: &impl Object,
        level_triggered: bool,
        signal: usize,
        pack: Box<dyn PackedSyscall>,
        waker: &Waker,
    ) -> Result {
        self.push_inner(
            |syscall| self.inner.push_raw(obj, level_triggered, signal, syscall),
            pack,
            waker,
        )
    }

    #[inline]
    pub(crate) fn push_chan_acrecv(
        &self,
        obj: &solvent::prelude::Channel,
        id: usize,
        pack: Box<dyn PackedSyscall>,
        waker: &Waker,
    ) -> Result {
        self.push_inner(
            |syscall| obj.call_receive_async(id, &self.inner, syscall),
            pack,
            waker,
        )
    }
}

pub trait PackedSyscall {
    fn raw(&self) -> Syscall;

    fn unpack(&mut self, result: usize, canceled: bool) -> Result;
}
