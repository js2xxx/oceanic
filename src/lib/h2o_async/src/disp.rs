use alloc::{boxed::Box, collections::BTreeMap};
use core::{
    mem,
    sync::atomic::{AtomicUsize, Ordering::*},
    task::{Poll, Waker},
};

use solvent::prelude::{Dispatcher as Inner, Object, Result, Syscall, ENOENT, EPIPE, ETIME};
use solvent_std::sync::{Arsc, Mutex};

struct Task {
    pack: Box<dyn PackedSyscall>,
    waker: Waker,
}

const NORMAL: usize = 0;
const DISCONNECTED: usize = isize::MAX as usize;

struct Dispatcher {
    inner: Inner,
    state: AtomicUsize,
    num_recv: AtomicUsize,
    tasks: Mutex<BTreeMap<usize, Task>>,
}

impl Dispatcher {
    #[inline]
    fn new(capacity: usize) -> Self {
        Dispatcher {
            inner: Inner::new(capacity),
            state: AtomicUsize::new(NORMAL),
            num_recv: AtomicUsize::new(1),
            tasks: Mutex::new(BTreeMap::new()),
        }
    }

    fn poll_receive(&self) -> Poll<Result> {
        match self.inner.pop_raw() {
            Ok(res) => {
                let Task { waker, mut pack } = self.tasks.lock().remove(&res.key).ok_or(ETIME)?;
                // We need to inform the task where an internal error occurred.
                let res = pack.unpack(res.result, res.canceled);
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

    fn send<K>(&self, key: K, pack: Box<dyn PackedSyscall>, waker: &Waker) -> Result
    where
        K: FnOnce(Option<&Syscall>) -> Result<usize>,
    {
        if self.state.load(SeqCst) == DISCONNECTED {
            return Err(EPIPE);
        }

        let syscall = pack.raw();
        let key = key(syscall.as_ref())?;
        let task = Task {
            pack,
            waker: waker.clone(),
        };
        self.tasks.lock().insert(key, task);
        Ok(())
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

    #[inline]
    pub fn send(
        &self,
        obj: &impl Object,
        level_triggered: bool,
        signal: usize,
        pack: Box<dyn PackedSyscall>,
        waker: &Waker,
    ) -> Result {
        self.disp.send(
            |syscall| {
                self.disp
                    .inner
                    .push_raw(obj, level_triggered, signal, syscall)
            },
            pack,
            waker,
        )
    }

    #[inline]
    pub(crate) fn send_chan_acrecv(
        &self,
        obj: &solvent::prelude::Channel,
        id: usize,
        pack: Box<dyn PackedSyscall>,
        waker: &Waker,
    ) -> Result {
        self.disp.send(
            |syscall| obj.call_receive_async(id, &self.disp.inner, syscall),
            pack,
            waker,
        )
    }
}

impl Drop for DispSender {
    fn drop(&mut self) {
        let state = self.disp.state.swap(DISCONNECTED, SeqCst);
        if state != DISCONNECTED {
            // TODO: Signal the receiver.
        }
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
            let tasks = mem::take(&mut *self.disp.tasks.lock());
            for (_, task) in tasks {
                let Task { mut pack, waker } = task;
                let _ = pack.unpack(0, true);
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

pub trait PackedSyscall {
    fn raw(&self) -> Option<Syscall>;

    fn unpack(&mut self, result: usize, canceled: bool) -> Result;
}
