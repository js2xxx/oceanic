use core::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::Future;
use solvent::{
    prelude::{Object, Result, SIG_GENERIC},
    time::Instant,
};

use crate::push_task;

type Inner = solvent::dev::Interrupt;

pub struct Interrupt {
    inner: Inner,
}

impl From<Inner> for Interrupt {
    #[inline]
    fn from(inner: Inner) -> Self {
        Interrupt { inner }
    }
}

impl Interrupt {
    #[inline]
    pub fn last_time(&self) -> Result<Instant> {
        self.inner.wait(Duration::ZERO)
    }

    #[inline]
    pub fn wait_until_async(&self, now: Instant) -> WaitUntil<'_> {
        WaitUntil { intr: self, now }
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

#[must_use]
pub struct WaitUntil<'a> {
    intr: &'a Interrupt,
    now: Instant,
}

impl Future for WaitUntil<'_> {
    type Output = Result<Instant>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let last_time = self.intr.last_time()?;

        if self.now > last_time {
            let waiter = self.intr.inner.try_wait_async(false, SIG_GENERIC)?;
            push_task(waiter, cx.waker());
            Poll::Pending
        } else {
            Poll::Ready(Ok(last_time))
        }
    }
}
