use alloc::boxed::Box;
use core::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::Future;
use solvent::{
    prelude::{Result, Syscall, EPIPE, ETIME, SIG_TIMER},
    time::{Instant, Timer as Inner},
};
use solvent_std::sync::{
    channel::{oneshot, oneshot_},
    Arsc,
};

use crate::disp::{Dispatcher, PackedSyscall};

pub struct Timer {
    inner: Inner,
    disp: Arsc<Dispatcher>,
}

impl Timer {
    #[inline]
    pub fn new(inner: Inner, disp: Arsc<Dispatcher>) -> Self {
        Timer { inner, disp }
    }

    #[inline]
    pub fn wait_until(&self, deadline: Instant) -> TimerWait {
        TimerWait {
            timer: self,
            deadline,
            result: None,
        }
    }

    #[inline]
    pub async fn wait(&self, duration: Duration) -> Result {
        self.wait_until(Instant::now() + duration).await
    }
}

struct PackedTimer;

impl PackedSyscall for (PackedTimer, oneshot_::Sender<Result>) {
    #[inline]
    fn raw(&self) -> Option<Syscall> {
        None
    }

    #[inline]
    fn unpack(&mut self, _: usize, canceled: bool) -> Result {
        let res = (!canceled).then_some(()).ok_or(ETIME);
        self.1.send(res).map_err(|_| EPIPE)
    }
}

pub struct TimerWait<'a> {
    timer: &'a Timer,
    deadline: Instant,
    result: Option<oneshot_::Receiver<Result>>,
}

impl Future for TimerWait<'_> {
    type Output = Result;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(res) = self.result.take().and_then(|rx| rx.recv().ok()) {
            return Poll::Ready(res);
        }

        match self.timer.inner.set_deadline(self.deadline) {
            Err(ETIME) => Poll::Ready(Ok(())),
            Err(err) => Poll::Ready(Err(err)),
            Ok(()) => {
                let (tx, rx) = oneshot();
                self.result = Some(rx);
                self.timer.disp.push(
                    &self.timer.inner,
                    true,
                    SIG_TIMER,
                    Box::new((PackedTimer, tx)),
                    cx.waker(),
                )?;

                Poll::Pending
            }
        }
    }
}
