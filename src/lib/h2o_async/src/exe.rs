mod enter;
mod park;

use alloc::vec::Vec;
use core::{
    iter,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering::*},
    task::{Context, Poll},
};

use async_task::{Runnable, Task};
use futures::{
    task::{FutureObj, Spawn, SpawnError},
    Future,
};
use solvent::prelude::EPIPE;
#[cfg(feature = "runtime")]
use solvent_core::{sync::Lazy, thread::available_parallelism, thread_local};
use solvent_core::{
    sync::{Arsc, Injector, Stealer, Worker},
    thread::{self, Backoff},
};

use crate::disp::{DispReceiver, DispSender};

struct Blocking<G>(Option<G>);

impl<G> Unpin for Blocking<G> {}

impl<G, U> Future for Blocking<G>
where
    G: FnOnce() -> U + Send + 'static,
{
    type Output = U;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        let func = self.0.take().expect("Cannot run a task twice");
        Poll::Ready(func())
    }
}

#[derive(Debug)]
pub struct ThreadPool {
    inner: Arsc<Inner>,
}

#[derive(Debug)]
struct Inner {
    global: Injector<Runnable>,
    stealers: Vec<Stealer<Runnable>>,
    count: AtomicUsize,
}

impl ThreadPool {
    pub fn new(num: usize) -> Self {
        log::trace!("solvent-async::exe: Create thread pool");
        let injector = Injector::new();
        let (workers, stealers) = (0..num).fold(
            (Vec::with_capacity(num), Vec::with_capacity(num)),
            |(mut workers, mut stealers), _| {
                let worker = Worker::new_fifo();
                let stealer = worker.stealer();
                workers.push(worker);
                stealers.push(stealer);
                (workers, stealers)
            },
        );
        let inner = Arsc::new(Inner {
            global: injector,
            stealers,
            count: AtomicUsize::new(1),
        });

        workers.into_iter().for_each(|worker| {
            let inner = inner.clone();
            thread::spawn(move || worker_thread(worker, inner));
        });
        ThreadPool { inner }
    }

    pub fn spawn<F, T>(&self, fut: F) -> Task<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let inner = self.inner.clone();
        let (runnable, task) = async_task::spawn(fut, move |t| inner.global.push(t));
        runnable.schedule();
        task
    }

    pub fn spawn_blocking<F, T>(&self, func: F) -> Task<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        self.spawn(Blocking(Some(func)))
    }

    pub fn dispatch(&self, capacity: usize) -> DispSender {
        let (tx, rx) = crate::disp::dispatch(capacity);
        let inner = self.inner.clone();
        log::trace!("solvent-async::exe: Dispatch I/O operations");
        thread::spawn(move || io_thread(rx, inner));
        tx
    }

    #[inline]
    pub fn block_on<F, G, T>(&self, gen: G) -> T
    where
        F: Future<Output = T> + Send + 'static,
        G: FnOnce(ThreadPool) -> F,
    {
        let fut = gen(self.clone());
        enter::enter().block_on(fut)
    }
}

impl Spawn for ThreadPool {
    #[inline]
    fn spawn_obj(&self, future: FutureObj<'static, ()>) -> Result<(), SpawnError> {
        self.spawn(future).detach();
        Ok(())
    }
}

impl Clone for ThreadPool {
    fn clone(&self) -> Self {
        let inner = self.inner.clone();
        inner.count.fetch_add(1, Release);
        ThreadPool { inner }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.inner.count.fetch_sub(1, Release);
    }
}

fn worker_thread(local: Worker<Runnable>, pool: Arsc<Inner>) {
    log::trace!(
        "solvent-async::exe: worker thread #{}",
        thread::current().id()
    );
    #[inline]
    fn next_task<T>(local: &Worker<T>, global: &Injector<T>, stealers: &[Stealer<T>]) -> Option<T> {
        local.pop().or_else(|| {
            iter::repeat_with(|| {
                global
                    .steal_batch_and_pop(local)
                    .or_else(|| stealers.iter().map(|s| s.steal()).collect())
            })
            .find(|s| !s.is_retry())
            .and_then(|s| s.success())
        })
    }

    let backoff = Backoff::new();
    loop {
        match next_task(&local, &pool.global, &pool.stealers) {
            Some(runnable) => {
                runnable.run();
                backoff.reset();
            }
            None => {
                if pool.count.load(Acquire) == 0 {
                    break;
                }
                log::trace!("W#{}: Waiting for next task...", thread::current().id());
                backoff.snooze()
            }
        }
    }
}

fn io_thread(rx: DispReceiver, pool: Arsc<Inner>) {
    log::trace!("solvent-async::exe: io thread #{}", thread::current().id());
    let backoff = Backoff::new();
    loop {
        match rx.poll_receive() {
            Poll::Ready(res) => match res {
                Ok(()) => backoff.reset(),
                Err(EPIPE) => break,
                Err(err) => log::warn!("Error while polling for dispatcher: {:?}", err),
            },
            Poll::Pending => {
                if pool.count.load(Acquire) == 0 {
                    break;
                }
                log::trace!("IO#{}: Waiting for next task...", thread::current().id());
                backoff.snooze()
            }
        }
    }
}

cfg_if::cfg_if! { if #[cfg(feature = "runtime")] {

static POOL: Lazy<ThreadPool> = Lazy::new(|| ThreadPool::new(available_parallelism().into()));
thread_local! {
    static DISP: DispSender = POOL.dispatch(4096);
}

#[inline]
pub fn spawn<F, T>(fut: F) -> Task<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    POOL.spawn(fut)
}

#[inline]
pub fn spawn_blocking<F, T>(func: F) -> Task<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    POOL.spawn_blocking(func)
}

#[inline]
pub fn dispatch() -> DispSender {
    DISP.with(|tx| tx.clone())
}

#[inline]
pub fn block_on<F, T>(fut: F) -> T
where
    F: Future<Output = T> + Send + 'static,
{
    POOL.block_on(|_| fut)
}

#[macro_export]
macro_rules! entry {
    ($func:ident, $std:path) => {
        mod __h2o_async_inner {
            fn main() {
                $crate::block_on(async { (super::$func)().await })
            }

            use $std as std;
            std::entry!(main);
        }
    };
}

} }
