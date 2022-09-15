use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use solvent::prelude::{Handle, Packet, PacketTyped, Result, ENOENT, SIG_READ};

use crate::push_task;

type Inner = solvent::ipc::Channel;

pub struct Channel {
    inner: Inner,
}

impl From<Inner> for Channel {
    #[inline]
    fn from(inner: Inner) -> Self {
        Channel { inner }
    }
}

impl Channel {
    #[inline]
    pub fn send_raw(&self, id: Option<usize>, buffer: &[u8], handles: &[Handle]) -> Result {
        self.inner.send_raw(id, buffer, handles)
    }

    #[inline]
    pub fn send_packet(&self, packet: &mut Packet) -> Result {
        self.send_raw(packet.id, &packet.buffer, &packet.handles)
            .map(|_| *packet = Default::default())
    }

    #[inline]
    pub fn send<T: PacketTyped>(&self, packet: T) -> Result {
        self.send_packet(&mut packet.into_packet())
    }

    #[inline]
    fn poll_receive(&self, packet: &mut Packet) -> Poll<Result> {
        match self.inner.receive_packet(packet) {
            Err(ENOENT) => Poll::Pending,
            res => Poll::Ready(res),
        }
    }

    #[inline]
    pub fn receive_async<'a>(&'a self, packet: &'a mut Packet) -> Receive<'a> {
        Receive {
            channel: self,
            packet,
        }
    }

    #[inline]
    pub async fn receive_packet(&self, packet: &mut Packet) -> Result {
        self.receive_async(packet).await
    }

    pub async fn try_receive<T: PacketTyped>(
        &self,
    ) -> Result<core::result::Result<T, (T::TryFromError, Packet)>> {
        let mut packet = Default::default();
        self.receive_packet(&mut packet).await?;
        match T::try_from_packet(&mut packet) {
            Ok(packet) => Ok(Ok(packet)),
            Err(err) => Ok(Err((err, packet))),
        }
    }

    pub async fn receive<T: PacketTyped>(&self) -> Result<T> {
        let mut packet = Packet::default();
        self.receive_packet(&mut packet).await?;
        T::try_from_packet(&mut packet).map_err(Into::into)
    }
}

impl Channel {
    #[inline]
    pub fn call_send_raw(&self, buffer: &[u8], handles: &[Handle]) -> Result<usize> {
        self.inner.call_send_raw(buffer, handles)
    }

    #[inline]
    pub fn call_send(&self, packet: &Packet) -> Result<usize> {
        self.inner.call_send(packet)
    }

    #[inline]
    fn poll_call_receive(&self, id: usize, packet: &mut Packet) -> Poll<Result> {
        match self.inner.call_receive(id, packet, Duration::ZERO) {
            Err(ENOENT) => Poll::Pending,
            res => Poll::Ready(res),
        }
    }

    #[inline]
    pub fn call_receive_async<'a>(&'a self, id: usize, packet: &'a mut Packet) -> CallReceive<'a> {
        CallReceive {
            channel: self,
            id,
            packet,
        }
    }

    #[inline]
    pub async fn call_receive(&self, id: usize, packet: &mut Packet) -> Result {
        self.call_receive_async(id, packet).await
    }

    #[inline]
    pub async fn call(&self, packet: &mut Packet) -> Result {
        let id = self.call_send(packet)?;
        self.call_receive(id, packet).await
    }

    pub async fn handle<F, R>(&self, handler: F) -> Result<R>
    where
        F: FnOnce(&mut Packet) -> Result<R>,
    {
        let mut packet = Packet::default();
        self.receive_packet(&mut packet).await?;
        let id = packet.id;
        let ret = handler(&mut packet)?;
        packet.id = id;
        self.send_packet(&mut packet)?;
        Ok(ret)
    }
}

#[must_use]
pub struct Receive<'a> {
    channel: &'a Channel,
    packet: &'a mut Packet,
}

impl Future for Receive<'_> {
    type Output = Result;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let ret = self.channel.poll_receive(self.packet);
        if ret.is_pending() {
            let key = crate::disp().push(&self.channel.inner, true, SIG_READ)?;
            push_task(key, cx.waker());
        }
        ret
    }
}

#[must_use]
pub struct CallReceive<'a> {
    channel: &'a Channel,
    id: usize,
    packet: &'a mut Packet,
}

impl Future for CallReceive<'_> {
    type Output = Result;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let ret = self.channel.poll_call_receive(self.id, self.packet);
        if ret.is_pending() {
            let key = self
                .channel
                .inner
                .call_receive_async2(self.id, crate::disp())?;
            push_task(key, cx.waker());
        }
        ret
    }
}
