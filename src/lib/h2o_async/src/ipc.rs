mod channel;

use core::{
    future::Future,
    num::NonZeroUsize,
    pin::Pin,
    task::{Context, Poll},
};

use solvent::{
    error::Result,
    prelude::{Object, Syscall, ENOENT, EPIPE, ETIME},
};
use solvent_core::{
    sync::channel::{oneshot, TryRecvError},
    thread::Backoff,
};

pub use self::channel::*;
use crate::disp::{DispSender, PackedSyscall};

pub trait AsyncObject: Object {
    type TryWait<'a>: Future<Output = Result<usize>> + 'a
    where
        Self: 'a;

    fn try_wait_with<'a>(
        &'a self,
        disp: &'a DispSender,
        level_triggered: bool,
        signal: usize,
    ) -> Self::TryWait<'a>;
}

impl<T: Object> AsyncObject for T {
    type TryWait<'a> = TryWait<'a, T> where T: 'a;

    #[inline]
    fn try_wait_with<'a>(
        &'a self,
        disp: &'a DispSender,
        level_triggered: bool,
        signal: usize,
    ) -> Self::TryWait<'a> {
        TryWait {
            obj: self,
            disp,
            level_triggered,
            signal,
            result: None,
            key: None,
        }
    }
}

pub struct PackWait;

unsafe impl PackedSyscall for (PackWait, oneshot::Sender<Result<usize>>) {
    #[inline]
    fn raw(&self) -> Option<Syscall> {
        None
    }

    fn unpack(&mut self, _: usize, signal: Option<NonZeroUsize>) -> Result {
        self.1
            .send(match signal {
                Some(signal) => Ok(signal.get()),
                None => Err(ETIME),
            })
            .map_err(|_| EPIPE)
    }
}

#[must_use]
pub struct TryWait<'a, T> {
    obj: &'a T,
    disp: &'a DispSender,
    level_triggered: bool,
    signal: usize,
    result: Option<oneshot::Receiver<Result<usize>>>,
    key: Option<usize>,
}

impl<'a, T: Object> Future for TryWait<'a, T> {
    type Output = Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(rx) = self.result.take() {
            match rx.try_recv() {
                Ok(result) => return Poll::Ready(result),
                Err(TryRecvError::Empty) => {
                    self.result = Some(rx);
                    if let Err(err) = self
                        .key
                        .ok_or(ENOENT)
                        .map(|key| self.disp.update(key, cx.waker()).expect("update"))
                    {
                        return Poll::Ready(Err(err));
                    }
                }
                Err(TryRecvError::Disconnected) => {}
            }
        }

        let backoff = Backoff::new();
        let (mut tx, rx) = oneshot();
        self.result = Some(rx);
        loop {
            match self.disp.poll_send(
                self.obj,
                self.level_triggered,
                self.signal,
                (PackWait, tx),
                cx.waker(),
            ) {
                Err(pack) => {
                    tx = pack.1;
                    backoff.snooze()
                }
                Ok(Err(err)) => panic!("poll send: {err:?}"),
                Ok(Ok(key)) => {
                    self.key = Some(key);
                    break Poll::Pending;
                }
            }
        }
    }
}
