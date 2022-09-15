use alloc::boxed::Box;
use core::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::Future;
use solvent::{
    prelude::{PackIntrWait, Result, SerdeReg, Syscall, EPIPE, SIG_GENERIC},
    time::Instant,
};
use solvent_std::sync::{
    channel::{oneshot, oneshot_},
    Arsc,
};

use crate::disp::{Dispatcher, PackedSyscall};

type Inner = solvent::dev::Interrupt;

pub struct Interrupt {
    inner: Inner,
    disp: Arsc<Dispatcher>,
}

impl Interrupt {
    #[inline]
    pub fn new(inner: Inner, disp: Arsc<Dispatcher>) -> Self {
        Interrupt { inner, disp }
    }

    #[inline]
    pub fn last_time(&self) -> Result<Instant> {
        self.inner.wait(Duration::ZERO)
    }

    #[inline]
    pub fn wait_until_async(&self, now: Instant) -> WaitUntil<'_> {
        WaitUntil {
            intr: self,
            now,
            result: None,
        }
    }

    #[inline]
    pub async fn wait_until(&self, now: Instant) -> Result<Instant> {
        self.wait_until_async(now).await
    }

    #[inline]
    pub async fn wait_next(&self) -> Result<Instant> {
        self.wait_until(Instant::now()).await
    }
}

impl PackedSyscall for (PackIntrWait, oneshot_::Sender<Instant>) {
    #[inline]
    fn raw(&self) -> Syscall {
        self.0.syscall
    }

    #[inline]
    fn unpack(&self, result: usize) -> Result {
        let res = self.0.receive(SerdeReg::decode(result))?;
        self.1.send(res).map_err(|_| EPIPE)
    }
}

#[must_use]
pub struct WaitUntil<'a> {
    intr: &'a Interrupt,
    now: Instant,
    result: Option<oneshot_::Receiver<Instant>>,
}

impl Future for WaitUntil<'_> {
    type Output = Result<Instant>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(last_time) = self.result.take().and_then(|rx| rx.recv().ok()) {
            return Poll::Ready(Ok(last_time));
        }

        let last_time = self.intr.last_time()?;

        if self.now > last_time {
            let pack = self.intr.inner.pack_wait(self.now - last_time)?;
            let (tx, rx) = oneshot();
            self.result = Some(rx);
            self.intr.disp.push(
                &self.intr.inner,
                true,
                SIG_GENERIC,
                Box::new((pack, tx)),
                cx.waker(),
            )?;
            Poll::Pending
        } else {
            Poll::Ready(Ok(last_time))
        }
    }
}
