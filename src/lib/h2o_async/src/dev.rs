use core::{
    future::Future,
    num::NonZeroUsize,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use solvent::{
    prelude::{PackIntrWait, Result, SerdeReg, Syscall, EPIPE, SIG_GENERIC},
    time::Instant,
};
use solvent_core::{sync::channel::oneshot, thread::Backoff};

use crate::disp::{DispSender, PackedSyscall};

type Inner = solvent::dev::Interrupt;

pub struct Interrupt {
    inner: Inner,
    disp: DispSender,
}

#[cfg(feature = "runtime")]
impl From<Inner> for Interrupt {
    #[inline]
    fn from(inner: Inner) -> Self {
        Self::new(inner)
    }
}

impl Interrupt {
    #[inline]
    #[cfg(feature = "runtime")]
    pub fn new(inner: Inner) -> Self {
        Self::with_disp(inner, crate::dispatch())
    }

    #[inline]
    pub fn with_disp(inner: Inner, disp: DispSender) -> Self {
        Interrupt { inner, disp }
    }

    #[inline]
    pub fn rebind(&mut self, disp: DispSender) {
        self.disp = disp
    }

    #[inline]
    pub fn last_time(&self) -> Result<Instant> {
        self.inner.wait(Duration::ZERO)
    }

    #[inline]
    pub fn wait_until(&self, deadline: Instant) -> WaitUntil<'_> {
        WaitUntil {
            intr: self,
            deadline,
            result: None,
            key: None,
        }
    }

    #[inline]
    pub async fn wait_next(&self) -> Result<Instant> {
        self.wait_until(Instant::now()).await
    }
}

unsafe impl PackedSyscall for (PackIntrWait, oneshot::Sender<Result<Instant>>) {
    #[inline]
    fn raw(&self) -> Option<Syscall> {
        Some(self.0.syscall)
    }

    #[inline]
    fn unpack(&mut self, result: usize, signal: Option<NonZeroUsize>) -> Result {
        let res = self.0.receive(SerdeReg::decode(result), signal.is_none());
        self.1.send(res).map_err(|_| EPIPE)
    }
}

#[must_use]
pub struct WaitUntil<'a> {
    intr: &'a Interrupt,
    deadline: Instant,
    result: Option<oneshot::Receiver<Result<Instant>>>,
    key: Option<usize>,
}

impl Future for WaitUntil<'_> {
    type Output = Result<Instant>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(Poll::Ready(value)) = crate::utils::simple_recv(&mut self.result) {
            return Poll::Ready(value);
        }

        if let Some(key) = self.key {
            self.intr.disp.update(key, cx.waker()).expect("update");
            return Poll::Pending;
        }

        let mut last_time = self.intr.last_time()?;
        if self.deadline <= last_time {
            return Poll::Ready(Ok(last_time));
        }

        let backoff = Backoff::new();
        let (mut tx, rx) = oneshot();
        self.result = Some(rx);
        loop {
            let pack = if self.deadline > last_time {
                let pack = self.intr.inner.pack_wait(self.deadline - last_time)?;
                let res = self.intr.disp.poll_send(
                    &self.intr.inner,
                    true,
                    SIG_GENERIC,
                    (pack, tx),
                    cx.waker(),
                );
                match res {
                    Err((_, pack)) => pack,
                    Ok(Err(err)) => panic!("poll send: {err:?}"),
                    Ok(Ok(key)) => {
                        self.key = Some(key);
                        break Poll::Pending;
                    }
                }
            } else {
                break Poll::Ready(Ok(last_time));
            };
            tx = pack;
            backoff.snooze();
            last_time = self.intr.last_time()?;
        }
    }
}
