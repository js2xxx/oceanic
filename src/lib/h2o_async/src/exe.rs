#![allow(clippy::duplicate_mod)]

#[cfg(feature = "runtime")]
mod enter;
#[cfg(feature = "runtime")]
mod park;

use alloc::collections::BTreeMap;
use core::{
    iter,
    marker::PhantomData,
    sync::atomic::{AtomicUsize, Ordering::*},
    task::Poll,
};

use async_task::{Runnable, Task};
use futures_lite::{future::yield_now, pin, stream, Future, FutureExt, StreamExt};
use solvent_core::sync::{Arsc, Injector, Lazy, Steal, Stealer, Worker};

use crate::{disp::DispReceiver, sync::RwLock};

struct Inner {
    global: Injector<Runnable>,
    stealers: RwLock<BTreeMap<usize, Stealer<Runnable>>>,
}

#[repr(transparent)]
pub struct Executor {
    inner: Lazy<Arsc<Inner>>,
}

impl Executor {
    pub const fn new() -> Self {
        #[inline(never)]
        fn lazy_new() -> Arsc<Inner> {
            Arsc::new(Inner {
                global: Injector::new(),
                stealers: RwLock::new(BTreeMap::new()),
            })
        }
        Executor {
            inner: Lazy::new(lazy_new),
        }
    }

    pub async fn run<T>(&self, fut: impl Future<Output = T> + 'static) -> T {
        fut.or(poller(self.inner.clone())).await
    }

    pub async fn clear(&self) {
        poller_cleared(self.inner.clone()).await
    }

    pub fn spawn<T>(&self, fut: impl Future<Output = T> + Send + 'static) -> Task<T>
    where
        T: Send + 'static,
    {
        let inner = self.inner.clone();
        let (runnable, task) = async_task::spawn(fut, move |task| inner.global.push(task));
        runnable.schedule();
        task
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        // log::debug!("Drop on EXE {:p}", self);
        if Lazy::is_initialized(&self.inner) {
            loop {
                match self.inner.global.steal() {
                    Steal::Empty => break,
                    Steal::Success(task) => task.waker().wake(),
                    Steal::Retry => {}
                }
            }
        }
    }
}

#[repr(transparent)]
pub struct LocalExecutor {
    exe: Executor,
    _marker: PhantomData<*mut ()>,
}

impl LocalExecutor {
    pub const fn new() -> Self {
        LocalExecutor {
            exe: Executor::new(),
            _marker: PhantomData,
        }
    }

    pub async fn run<T>(&self, fut: impl Future<Output = T> + 'static) -> T {
        self.exe.run(fut).await
    }

    pub async fn clear(&self) {
        self.exe.clear().await
    }

    pub fn spawn<T: 'static>(&self, fut: impl Future<Output = T> + 'static) -> Task<T> {
        let inner = self.exe.inner.clone();
        // SAFETY: The executor is not `Send`, so the future doesn't need to be `Send`.
        let (runnable, task) =
            unsafe { async_task::spawn_unchecked(fut, move |task| inner.global.push(task)) };
        runnable.schedule();
        task
    }
}

impl Default for LocalExecutor {
    fn default() -> Self {
        Self::new()
    }
}

async fn tick(inner: &Inner, local: &Worker<Runnable>) -> bool {
    let stream = stream::iter(iter::repeat_with(|| {
        inner.global.steal_batch_and_pop(local)
    }))
    .then(|steal| async {
        let steal_from_others = async {
            let stealers = inner.stealers.read().await;
            stealers.values().map(Stealer::steal).collect()
        };
        match steal {
            Steal::Empty => steal_from_others.await,
            Steal::Success(_) => steal,
            Steal::Retry => match steal_from_others.await {
                Steal::Success(res) => Steal::Success(res),
                _ => Steal::Retry,
            },
        }
    });
    pin!(stream);

    let task = match local.pop() {
        Some(task) => Some(task),
        None => stream
            .find(|steal| !steal.is_retry())
            .await
            .and_then(|steal| steal.success()),
    };

    match task {
        Some(task) => {
            task.run();
            true
        }
        None => false,
    }
}

static ID: AtomicUsize = AtomicUsize::new(1);

async fn poller<T>(inner: Arsc<Inner>) -> T {
    let local = Worker::new_fifo();

    let mut stealers = inner.stealers.write().await;
    let id = ID.fetch_add(1, SeqCst);
    assert!(id != 0);
    stealers.insert(id, local.stealer());
    drop(stealers);

    loop {
        if !tick(&inner, &local).await {
            yield_now().await
        }
    }
}

async fn poller_cleared(inner: Arsc<Inner>) {
    let local = Worker::new_fifo();

    let mut stealers = inner.stealers.write().await;
    let id = ID.fetch_add(1, SeqCst);
    assert!(id != 0);
    stealers.insert(id, local.stealer());
    drop(stealers);

    loop {
        if !tick(&inner, &local).await {
            break;
        }
    }

    let mut stealers = inner.stealers.write().await;
    stealers.remove(&id);
}

pub async fn io_task(rx: DispReceiver) {
    loop {
        if let Poll::Ready(Err(e)) = rx.poll_receive() {
            log::trace!("IO task polled error: {e:?}");
        }
        yield_now().await
    }
}

#[cfg(feature = "runtime")]
pub(crate) mod runtime {

    use futures_lite::future::pending;
    use solvent_core::{
        thread::{self, available_parallelism},
        thread_local,
    };

    use crate::{disp::DispSender, exe::*, sync::channel};

    static GLOBAL: Executor = Executor::new();

    thread_local! {
        static LOCAL: LocalExecutor = LocalExecutor::new();
    }

    #[inline]
    pub fn spawn_local<T: 'static>(fut: impl Future<Output = T> + 'static) -> Task<T> {
        LOCAL.with(|local| local.spawn(fut))
    }

    #[inline]
    pub fn spawn<T: Send + 'static>(fut: impl Future<Output = T> + Send + 'static) -> Task<T> {
        GLOBAL.spawn(fut)
    }

    pub fn block_on<T: 'static>(num: Option<usize>, fut: impl Future<Output = T> + 'static) -> T {
        let num = num
            .unwrap_or_else(|| available_parallelism().get())
            .saturating_sub(1);

        let tx = (num > 0).then(|| {
            let (tx, rx) = channel::bounded(num);

            for _ in 0..num {
                let rx = rx.clone();
                thread::spawn(move || {
                    let stop = async move {
                        let _ = rx.recv().await;
                    };
                    LOCAL.with(|local| {
                        let local = local.run(stop);
                        let global = GLOBAL.run(pending());
                        let fut = local.or(global);

                        enter::enter().block_on(fut);
                    });
                });
            }
            tx
        });

        LOCAL.with(|local| {
            let local = local.run(fut);
            let global = GLOBAL.run(pending());
            let fut = local.or(global);

            enter::enter().block_on(async {
                let ret = fut.await;
                if let Some(tx) = tx {
                    for _ in 0..num {
                        let _ = tx.send(()).await;
                    }
                }
                ret
            })
        })
    }

    static DISP: Lazy<DispSender> = Lazy::new(|| {
        let (tx, rx) = crate::disp::dispatch(4096);
        spawn(io_task(rx)).detach();
        tx
    });

    #[inline]
    pub fn dispatch() -> DispSender {
        DISP.clone()
    }

    #[macro_export]
    macro_rules! entry {
        ($func:ident, $std:path, $num:expr) => {
            mod __h2o_async_inner {
                fn main() {
                    $crate::block_on($num, async { (super::$func)().await })
                }

                use $std as std;
                std::entry!(main);
            }
        };
    }
}
