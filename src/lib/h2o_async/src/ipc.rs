use alloc::boxed::Box;
use core::{
    future::Future,
    mem,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use solvent::prelude::{
    Handle, PackRecv, Packet, PacketTyped, Result, SerdeReg, Syscall, EBUFFER, ENOENT, EPIPE,
    SIG_READ,
};
use solvent_std::sync::{
    channel::{oneshot, oneshot_},
    Arsc,
};

use crate::disp::{Dispatcher, PackedSyscall};

type Inner = solvent::ipc::Channel;

pub struct Channel {
    inner: Inner,
    disp: Arsc<Dispatcher>,
}

impl Channel {
    #[inline]
    pub fn new(inner: Inner, disp: Arsc<Dispatcher>) -> Self {
        Channel { inner, disp }
    }

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
    pub fn receive_with(&self, packet: Packet) -> Receive {
        Receive {
            channel: self,
            packet,
            result: None,
        }
    }

    pub async fn receive_packet(&self, packet: &mut Packet) -> Result {
        let temp = mem::take(packet);
        let temp = self.receive_with(temp).await?;
        *packet = temp;
        Ok(())
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
    pub fn call_receive_with(&self, id: usize, packet: Packet) -> CallReceive {
        CallReceive {
            channel: self,
            id,
            packet,
            result: None,
        }
    }

    pub async fn call_receive(&self, id: usize, packet: &mut Packet) -> Result {
        let temp = mem::take(packet);
        let temp = self.call_receive_with(id, temp).await?;
        *packet = temp;
        Ok(())
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

pub(crate) struct SendData {
    pub id: Result<usize>,
    pub buffer_size: usize,
    pub handle_count: usize,
    pub packet: Packet,
}

impl PackedSyscall for (PackRecv, oneshot_::Sender<SendData>) {
    #[inline]
    fn raw(&self) -> Option<Syscall> {
        Some(self.0.syscall)
    }

    fn unpack(&mut self, result: usize, canceled: bool) -> Result {
        let (id, buffer_size, handle_count) = self.0.receive(SerdeReg::decode(result), canceled);
        self.1
            .send(SendData {
                id,
                buffer_size,
                handle_count,
                packet: mem::take(&mut self.0.packet),
            })
            .map_err(|_| EPIPE)
    }
}

#[must_use]
pub struct Receive<'a> {
    channel: &'a Channel,
    packet: Packet,
    result: Option<oneshot_::Receiver<SendData>>,
}

impl<'a> Future for Receive<'a> {
    type Output = Result<Packet>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<Packet>> {
        let mut packet = match self.result.take().and_then(|rx| rx.recv().ok()) {
            Some(send_data) => match send_data.id {
                Ok(id) => {
                    let mut packet = send_data.packet;
                    packet.id = Some(id);
                    return Poll::Ready(Ok(packet));
                }
                Err(EBUFFER) => {
                    let mut packet = send_data.packet;
                    packet.buffer.reserve(send_data.buffer_size);
                    packet.handles.reserve(send_data.handle_count);
                    packet
                }
                Err(err) => return Poll::Ready(Err(err)),
            },
            None => mem::take(&mut self.packet),
        };

        match self.channel.inner.receive_packet(&mut packet) {
            Err(ENOENT) => {
                let pack = self.channel.inner.pack_receive(packet);
                let (tx, rx) = oneshot();
                self.result = Some(rx);
                self.channel.disp.push(
                    &self.channel.inner,
                    true,
                    SIG_READ,
                    Box::new((pack, tx)),
                    cx.waker(),
                )?;
                Poll::Pending
            }
            res => Poll::Ready(res.map(|_| packet)),
        }
    }
}

#[must_use]
pub struct CallReceive<'a> {
    channel: &'a Channel,
    id: usize,
    packet: Packet,
    result: Option<oneshot_::Receiver<SendData>>,
}

impl Future for CallReceive<'_> {
    type Output = Result<Packet>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut packet = match self.result.take().and_then(|rx| rx.recv().ok()) {
            Some(send_data) => match send_data.id {
                Ok(id) => {
                    let mut packet = send_data.packet;
                    packet.id = Some(id);
                    return Poll::Ready(Ok(packet));
                }
                Err(EBUFFER) => {
                    let mut packet = send_data.packet;
                    packet.buffer.reserve(send_data.buffer_size);
                    packet.handles.reserve(send_data.handle_count);
                    packet
                }
                Err(err) => return Poll::Ready(Err(err)),
            },
            None => mem::take(&mut self.packet),
        };

        match self
            .channel
            .inner
            .call_receive(self.id, &mut packet, Duration::ZERO)
        {
            Err(ENOENT) => {
                let pack = self.channel.inner.pack_call_receive(self.id, packet);
                let (tx, rx) = oneshot();
                self.result = Some(rx);
                self.channel.disp.push_chan_acrecv(
                    &self.channel.inner,
                    self.id,
                    Box::new((pack, tx)),
                    cx.waker(),
                )?;
                Poll::Pending
            }
            res => Poll::Ready(res.map(|_| packet)),
        }
    }
}
