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
                pack.unpack(res.result)?;
                waker.wake();
                Poll::Ready(Ok(()))
            }
            Err(ENOENT) => Poll::Pending,
            Err(err) => Err(err)?,
        }
    }

    pub fn push(
        &self,
        obj: &impl Object,
        level_triggered: bool,
        signal: usize,
        pack: Box<dyn PackedSyscall>,
        waker: &Waker,
    ) -> Result {
        let syscall = pack.raw();
        let key = self
            .inner
            .push_raw(obj, level_triggered, signal, &syscall)?;
        let task = Task {
            pack,
            waker: AtomicWaker::new(),
        };
        task.waker.register(waker);
        self.tasks.lock().insert(key, task);
        Ok(())
    }

    pub fn push_chan_acrecv(
        &self,
        obj: &solvent::prelude::Channel,
        id: usize,
        pack: Box<dyn PackedSyscall>,
        waker: &Waker,
    ) -> Result {
        let syscall = pack.raw();
        let key = obj.call_receive_async(id, &self.inner, &syscall)?;
        let task = Task {
            pack,
            waker: AtomicWaker::new(),
        };
        task.waker.register(waker);
        self.tasks.lock().insert(key, task);
        Ok(())
    }
}

pub trait PackedSyscall {
    fn raw(&self) -> Syscall;

    fn unpack(&mut self, result: usize) -> Result;
}
