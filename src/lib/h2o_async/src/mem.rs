use alloc::vec::Vec;
use core::{
    future::Future,
    mem,
    num::NonZeroUsize,
    ops::ControlFlow,
    pin::Pin,
    task::{Context, Poll},
};

use solvent::prelude::{
    IoSlice, IoSliceMut, PackRead, PackResize, PackWrite, Result, SerdeReg, Syscall, EAGAIN,
    ENOENT, EPIPE, SIG_READ, SIG_WRITE,
};
use solvent_core::{
    sync::channel::{oneshot, TryRecvError},
    thread::Backoff,
};

use crate::disp::{DispSender, PackedSyscall};

type Inner = solvent::mem::Phys;

pub struct Phys {
    inner: Inner,
    disp: DispSender,
}

#[cfg(feature = "runtime")]
impl From<Inner> for Phys {
    #[inline]
    fn from(inner: Inner) -> Self {
        Self::new(inner)
    }
}

impl AsRef<Inner> for Phys {
    #[inline]
    fn as_ref(&self) -> &Inner {
        &self.inner
    }
}

impl From<Phys> for Inner {
    #[inline]
    fn from(value: Phys) -> Self {
        value.inner
    }
}

impl Phys {
    #[cfg(feature = "runtime")]
    pub fn new(inner: Inner) -> Phys {
        Self::with_disp(inner, crate::dispatch())
    }

    #[inline]
    pub fn with_disp(inner: Inner, disp: DispSender) -> Phys {
        Phys { inner, disp }
    }

    #[inline]
    pub fn into_inner(this: Self) -> Inner {
        this.inner
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline]
    pub fn read_with(&self, offset: usize, buf: Vec<u8>) -> Read {
        Read {
            offset,
            buf,
            result: None,
            f: FutInner {
                phys: self,
                key: None,
            },
        }
    }

    pub async fn read(&self, offset: usize, buf: &mut Vec<u8>) -> Result {
        let b = mem::take(buf);
        let b = self.read_with(offset, b).await?;
        *buf = b;
        Ok(())
    }

    /// # Safety
    ///
    /// The caller must guarantee the memory safety of sharing the object.
    #[inline]
    pub unsafe fn write_with(&self, offset: usize, buf: Vec<u8>) -> Write {
        Write {
            offset,
            buf,
            result: None,
            f: FutInner {
                phys: self,
                key: None,
            },
        }
    }

    /// Note: The buffer is actually not modified. Passing its mutable reference
    /// is to transfer its ownership amidst the async context and avoid copying.
    ///
    /// # Safety
    ///
    /// The caller must guarantee the memory safety of sharing the object.
    pub async unsafe fn write(&self, offset: usize, buf: &mut Vec<u8>) -> Result<usize> {
        let b = mem::take(buf);
        let (b, len) = self.write_with(offset, b).await?;
        *buf = b;
        Ok(len)
    }

    #[inline]
    pub fn resize(&self, new_len: usize, zeroed: bool) -> Resize {
        Resize {
            new_len,
            zeroed,
            result: None,
            f: FutInner {
                phys: self,
                key: None,
            },
        }
    }
}

unsafe impl PackedSyscall for (PackRead, oneshot::Sender<Result<Vec<u8>>>) {
    #[inline]
    fn raw(&self) -> Option<Syscall> {
        Some(self.0.syscall)
    }

    fn unpack(&mut self, result: usize, signal: Option<NonZeroUsize>) -> Result {
        let len = self.0.receive(SerdeReg::decode(result), signal.is_none());
        self.1
            .send(len.map(|len| {
                let mut buf = mem::take(&mut self.0.buf);
                unsafe { buf.set_len(len) };
                buf
            }))
            .map_err(|_| EPIPE)
    }
}

struct FutInner<'a> {
    phys: &'a Phys,
    key: Option<usize>,
}

impl<'a> FutInner<'a> {
    fn result_recv<T>(
        &self,
        result: &Option<oneshot::Receiver<Result<T>>>,
        cx: &mut Context,
    ) -> ControlFlow<Poll<Result<T>>> {
        match result {
            Some(rx) => match rx.try_recv() {
                // Has a result
                Ok(res) => match res {
                    // The lock is already taken, restart
                    Err(EAGAIN) => ControlFlow::CONTINUE,
                    res => ControlFlow::Break(Poll::Ready(res)),
                },

                // Not yet, continue waiting
                Err(TryRecvError::Empty) => {
                    let Some(key) = self.key else {
                        return ControlFlow::Break(Poll::Ready(Err(ENOENT)))
                    };

                    if let Err(err) = self.phys.disp.update(key, cx.waker()) {
                        if let Ok(res) = rx.recv() {
                            return ControlFlow::Break(Poll::Ready(res));
                        }
                        panic!("Update future error with key {key}: {err:?}");
                    }

                    ControlFlow::Break(Poll::Pending)
                }

                // Channel early disconnected, restart the default process
                Err(TryRecvError::Disconnected) => ControlFlow::CONTINUE,
            },
            None => ControlFlow::CONTINUE,
        }
    }
}

#[must_use]
pub struct Read<'a> {
    result: Option<oneshot::Receiver<Result<Vec<u8>>>>,
    offset: usize,
    buf: Vec<u8>,
    f: FutInner<'a>,
}

impl Future for Read<'_> {
    type Output = Result<Vec<u8>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut buf = match self.f.result_recv(&self.result, cx) {
            ControlFlow::Continue(()) => mem::take(&mut self.buf),
            ControlFlow::Break(value) => return value,
        };

        let backoff = Backoff::new();
        let (mut tx, rx) = oneshot();
        self.result = Some(rx);
        loop {
            let mut vec = [IoSliceMut::uninit(buf.spare_capacity_mut())];
            match self.f.phys.inner.read_vectored(self.offset, &mut vec) {
                Err(EAGAIN) => {
                    match self.f.phys.disp.poll_send(
                        &self.f.phys.inner,
                        true,
                        SIG_READ,
                        (self.f.phys.inner.pack_read(self.offset, buf), tx),
                        cx.waker(),
                    ) {
                        Err(pack) => (buf, tx) = (pack.0.buf, pack.1),
                        Ok(Err(err)) => panic!("poll send: {err:?}"),
                        Ok(Ok(key)) => {
                            self.f.key = Some(key);
                            return Poll::Pending;
                        }
                    }
                }
                res => {
                    return Poll::Ready(res.map(|len| {
                        unsafe { buf.set_len(len) };
                        buf
                    }))
                }
            }
            backoff.snooze()
        }
    }
}

unsafe impl PackedSyscall for (PackWrite, oneshot::Sender<Result<(Vec<u8>, usize)>>) {
    #[inline]
    fn raw(&self) -> Option<Syscall> {
        Some(self.0.syscall)
    }

    fn unpack(&mut self, result: usize, signal: Option<NonZeroUsize>) -> Result {
        let len = self.0.receive(SerdeReg::decode(result), signal.is_none());
        self.1
            .send(len.map(|len| (mem::take(&mut self.0.buf), len)))
            .map_err(|_| EPIPE)
    }
}

#[must_use]
pub struct Write<'a> {
    offset: usize,
    buf: Vec<u8>,
    #[allow(clippy::type_complexity)]
    result: Option<oneshot::Receiver<Result<(Vec<u8>, usize)>>>,
    f: FutInner<'a>,
}

impl Future for Write<'_> {
    type Output = Result<(Vec<u8>, usize)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut buf = match self.f.result_recv(&self.result, cx) {
            ControlFlow::Continue(()) => mem::take(&mut self.buf),
            ControlFlow::Break(value) => return value,
        };

        let backoff = Backoff::new();
        let (mut tx, rx) = oneshot();
        self.result = Some(rx);
        loop {
            let vec = [IoSlice::new(&buf)];
            match unsafe { self.f.phys.inner.write_vectored(self.offset, &vec) } {
                Err(EAGAIN) => match self.f.phys.disp.poll_send(
                    &self.f.phys.inner,
                    true,
                    SIG_WRITE,
                    (self.f.phys.inner.pack_write(self.offset, buf), tx),
                    cx.waker(),
                ) {
                    Err(pack) => (buf, tx) = (pack.0.buf, pack.1),
                    Ok(Err(err)) => panic!("poll send: {err:?}"),
                    Ok(Ok(key)) => {
                        self.f.key = Some(key);
                        return Poll::Pending;
                    }
                },
                res => return Poll::Ready(res.map(|len| (buf, len))),
            }
            backoff.snooze()
        }
    }
}

unsafe impl PackedSyscall for (PackResize, oneshot::Sender<Result>) {
    #[inline]
    fn raw(&self) -> Option<Syscall> {
        Some((self.0).0)
    }

    fn unpack(&mut self, result: usize, signal: Option<NonZeroUsize>) -> Result {
        let res = self.0.receive(SerdeReg::decode(result), signal.is_none());
        self.1.send(res).map_err(|_| EPIPE)
    }
}

#[must_use]
pub struct Resize<'a> {
    new_len: usize,
    zeroed: bool,
    result: Option<oneshot::Receiver<Result>>,
    f: FutInner<'a>,
}

impl Future for Resize<'_> {
    type Output = Result;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let ControlFlow::Break(res) = self.f.result_recv(&self.result, cx) {
            return res;
        }

        let backoff = Backoff::new();
        let (mut tx, rx) = oneshot();
        self.result = Some(rx);
        loop {
            match self.f.phys.inner.resize(self.new_len, self.zeroed) {
                Err(EAGAIN) => match self.f.phys.disp.poll_send(
                    &self.f.phys.inner,
                    true,
                    SIG_WRITE,
                    (self.f.phys.inner.pack_resize(self.new_len, self.zeroed), tx),
                    cx.waker(),
                ) {
                    Err(pack) => tx = pack.1,
                    Ok(Err(err)) => panic!("poll send: {err:?}"),
                    Ok(Ok(key)) => {
                        self.f.key = Some(key);
                        return Poll::Pending;
                    }
                },
                res => return Poll::Ready(res),
            }
            backoff.snooze()
        }
    }
}
