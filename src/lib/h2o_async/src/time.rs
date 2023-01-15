use core::time::Duration;

use futures_lite::{stream, Stream};
use solvent::{
    error::Result,
    prelude::SIG_TIMER,
    time::{Instant, Timer as Inner},
};

use crate::{disp::DispSender, ipc::AsyncObject};

pub struct Timer {
    inner: Inner,
    disp: DispSender,
}

#[cfg(feature = "runtime")]
impl From<Inner> for Timer {
    #[inline]
    fn from(inner: Inner) -> Self {
        Self::new(inner)
    }
}

impl AsRef<Inner> for Timer {
    #[inline]
    fn as_ref(&self) -> &Inner {
        &self.inner
    }
}

impl From<Timer> for Inner {
    #[inline]
    fn from(value: Timer) -> Self {
        value.inner
    }
}

impl Timer {
    #[inline]
    #[cfg(feature = "runtime")]
    pub fn new(inner: Inner) -> Self {
        Self::with_disp(inner, crate::dispatch())
    }

    #[inline]
    pub fn with_disp(inner: Inner, disp: DispSender) -> Self {
        Timer { inner, disp }
    }

    #[inline]
    pub fn into_inner(this: Self) -> Inner {
        this.inner
    }

    #[inline]
    pub fn rebind(&mut self, disp: DispSender) {
        self.disp = disp
    }

    #[inline]
    pub fn reset(&self) -> Result {
        self.inner.reset()
    }

    pub async fn wait_after(&self, duration: Duration) -> Result {
        self.inner.set(duration)?;
        AsyncObject::try_wait_with(&self.inner, &self.disp, false, SIG_TIMER).await?;
        Ok(())
    }

    pub async fn wait_until(&self, deadline: Instant) -> Result {
        self.inner.set_deadline(deadline)?;
        AsyncObject::try_wait_with(&self.inner, &self.disp, false, SIG_TIMER).await?;
        Ok(())
    }

    #[inline]
    pub fn interval(&self, period: Duration) -> impl Stream<Item = Result> + '_ {
        stream::unfold((), move |_| async move {
            Some((self.wait_after(period).await, ()))
        })
    }
}
