use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_task::Runnable;
use crossbeam_queue::SegQueue;
use futures_lite::{future::yield_now, Future};
use solvent_async::{disp::DispSender, sync::channel::Sender};
use solvent_core::sync::Arsc;

struct Data {
    task: Runnable,
    stop: Option<Sender<()>>,
}

struct Inner {
    queue: SegQueue<Data>,
    disp: DispSender,
    stopped: AtomicBool,
    spawner_count: AtomicUsize,
}

pub struct Spawner {
    inner: Arsc<Inner>,
}

impl Clone for Spawner {
    fn clone(&self) -> Self {
        let inner = self.inner.clone();
        inner.spawner_count.fetch_add(1, Ordering::SeqCst);
        Spawner { inner }
    }
}

impl Drop for Spawner {
    fn drop(&mut self) {
        if self.inner.spawner_count.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.stop()
        }
    }
}

impl Spawner {
    pub fn new(disp: DispSender) -> Self {
        Spawner {
            inner: Arsc::new(Inner {
                queue: SegQueue::new(),
                disp,
                stopped: AtomicBool::new(false),
                spawner_count: AtomicUsize::new(1),
            }),
        }
    }

    pub fn is_stopped(&self) -> bool {
        self.inner.stopped.load(Ordering::Acquire)
    }

    pub fn runner(&self) -> Runner {
        Runner {
            inner: self.inner.clone(),
        }
    }

    pub fn dispatch(&self) -> DispSender {
        self.inner.disp.clone()
    }

    pub fn spawn(&self, fut: impl Future<Output = ()> + Send + 'static) {
        if !self.is_stopped() {
            let i2 = self.inner.clone();
            let (task, handle) =
                async_task::spawn(fut, move |task| i2.queue.push(Data { task, stop: None }));
            task.schedule();
            handle.detach();
        }
    }

    pub fn spawn_stoppable(
        &self,
        fut: impl Future<Output = ()> + Send + 'static,
        stop: Sender<()>,
    ) {
        if !self.is_stopped() {
            let i2 = self.inner.clone();
            let (task, handle) = async_task::spawn(fut, move |task| {
                i2.queue.push(Data {
                    task,
                    stop: Some(stop.clone()),
                })
            });
            task.schedule();
            handle.detach();
        }
    }

    pub fn stop(&self) {
        if !self.inner.stopped.swap(true, Ordering::AcqRel) {
            let len = self.inner.queue.len();
            for _ in 0..len {
                if let Some(mut data) = self.inner.queue.pop() {
                    if let Some(stop) = data.stop.take() {
                        let _ = stop.send_blocking(());
                    }
                    self.inner.queue.push(data)
                }
            }
        }
    }
}

pub struct Runner {
    inner: Arsc<Inner>,
}

impl Runner {
    pub async fn run(self) {
        loop {
            if self.inner.stopped.load(Ordering::Acquire) && self.inner.queue.is_empty() {
                break;
            }
            if let Some(data) = self.inner.queue.pop() {
                data.task.run();
            }
            yield_now().await
        }
    }
}

#[cfg(feature = "runtime")]
pub fn spawner() -> Spawner {
    let disp = Spawner::new(solvent_async::dispatch());
    solvent_async::spawn(disp.runner().run()).detach();
    disp
}
