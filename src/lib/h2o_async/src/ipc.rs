use core::{
    future::Future,
    mem,
    ops::ControlFlow,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use solvent::prelude::{
    Handle, PackRecv, Packet, PacketTyped, Result, SerdeReg, Syscall, EBUFFER, ENOENT, EPIPE,
    SIG_READ,
};
use solvent_std::{
    sync::channel::{oneshot, oneshot_, TryRecvError},
    thread::Backoff,
};

use crate::disp::{DispSender, PackedSyscall};

type Inner = solvent::ipc::Channel;

pub struct Channel {
    inner: Inner,
    disp: DispSender,
}

impl Channel {
    #[inline]
    pub fn new(inner: Inner, disp: DispSender) -> Self {
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
            key: None,
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
            id,
            recv: Receive {
                channel: self,
                packet,
                result: None,
                key: None,
            },
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

    pub async fn handle<G, F, R>(&self, handler: G) -> Result<R>
    where
        G: FnOnce(Packet) -> F,
        F: Future<Output = Result<(R, Packet)>>,
    {
        let mut packet = Packet::default();
        self.receive_packet(&mut packet).await?;
        let id = packet.id;
        let (ret, mut packet) = handler(packet).await?;
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

unsafe impl PackedSyscall for (PackRecv, oneshot_::Sender<SendData>) {
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
    key: Option<usize>,
}

impl<'a> Receive<'a> {
    fn result_recv(&mut self, cx: &mut Context<'_>) -> ControlFlow<Poll<Result<Packet>>, Packet> {
        let packet = match self.result.take() {
            Some(rx) => match rx.try_recv() {
                // Has a result
                Ok(send_data) => match send_data.id {
                    // Packet transferring successful, return it
                    Ok(id) => {
                        let mut packet = send_data.packet;
                        packet.id = Some(id);
                        return ControlFlow::Break(Poll::Ready(Ok(packet)));
                    }

                    // Packet buffer too small, reserve enough memory and restart polling
                    Err(EBUFFER) => {
                        let mut packet = send_data.packet;
                        packet.buffer.reserve(send_data.buffer_size);
                        packet.handles.reserve(send_data.handle_count);
                        Some(packet)
                    }

                    // Actual error occurred, return it
                    Err(err) => return ControlFlow::Break(Poll::Ready(Err(err))),
                },

                // Not yet, continue waiting
                Err(TryRecvError::Empty) => {
                    self.result = Some(rx);
                    if let Err(err) = self
                        .key
                        .ok_or(ENOENT)
                        .and_then(|key| self.channel.disp.update(key, cx.waker()))
                    {
                        return ControlFlow::Break(Poll::Ready(Err(err)));
                    }

                    return ControlFlow::Break(Poll::Pending);
                }

                // Channel early disconnected, restart the default process
                Err(TryRecvError::Disconnected) => None,
            },

            _ => None,
        };

        self.key = None;
        ControlFlow::Continue(packet.unwrap_or_else(|| mem::take(&mut self.packet)))
    }

    #[inline]
    fn poll_inner<Recv, PackSend>(
        &mut self,
        mut packet: Packet,
        recv: Recv,
        pack_send: PackSend,
    ) -> ControlFlow<Poll<Result<Packet>>, (Packet, oneshot_::Sender<SendData>)>
    where
        Recv: FnOnce(&mut Self, &mut Packet) -> Result,
        PackSend:
            FnOnce(
                &mut Self,
                Packet,
            )
                -> core::result::Result<Result<usize>, (PackRecv, oneshot_::Sender<SendData>)>,
    {
        match recv(self, &mut packet) {
            Err(ENOENT) => match pack_send(self, packet) {
                Err(pack) => ControlFlow::Continue((pack.0.packet, pack.1)),
                Ok(Err(err)) => ControlFlow::Break(Poll::Ready(Err(err))),
                Ok(Ok(key)) => {
                    self.key = Some(key);
                    ControlFlow::Break(Poll::Pending)
                }
            },
            res => ControlFlow::Break(Poll::Ready(res.map(|_| packet))),
        }
    }
}

impl<'a> Future for Receive<'a> {
    type Output = Result<Packet>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<Packet>> {
        let mut packet = match self.result_recv(cx) {
            ControlFlow::Continue(packet) => packet,
            ControlFlow::Break(res) => return res,
        };

        let backoff = Backoff::new();
        let (mut tx, rx) = oneshot();
        self.result = Some(rx);
        loop {
            let cf = self.poll_inner(
                packet,
                |r, packet| r.channel.inner.receive_packet(packet),
                |r, packet| {
                    r.channel.disp.poll_send(
                        &r.channel.inner,
                        true,
                        SIG_READ,
                        (r.channel.inner.pack_receive(packet), tx),
                        cx.waker(),
                    )
                },
            );
            (packet, tx) = match cf {
                ControlFlow::Break(res) => break res,
                ControlFlow::Continue(res) => res,
            };
            backoff.snooze()
        }
    }
}

#[must_use]
pub struct CallReceive<'a> {
    id: usize,
    recv: Receive<'a>,
}

impl Future for CallReceive<'_> {
    type Output = Result<Packet>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut packet = match self.recv.result_recv(cx) {
            ControlFlow::Continue(packet) => packet,
            ControlFlow::Break(res) => return res,
        };

        let backoff = Backoff::new();
        let (mut tx, rx) = oneshot();
        self.recv.result = Some(rx);
        loop {
            let id = self.id;
            let cf = self.recv.poll_inner(
                packet,
                |r, packet| r.channel.inner.call_receive(id, packet, Duration::ZERO),
                |r, packet| {
                    r.channel.disp.poll_chan_acrecv(
                        &r.channel.inner,
                        id,
                        (r.channel.inner.pack_call_receive(id, packet), tx),
                        cx.waker(),
                    )
                },
            );
            (packet, tx) = match cf {
                ControlFlow::Break(res) => break res,
                ControlFlow::Continue(res) => res,
            };
            backoff.snooze()
        }
    }
}
