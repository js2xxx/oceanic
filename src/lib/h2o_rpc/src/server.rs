use core::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering::*},
    task::{ready, Context, Poll},
};

use futures::{pin_mut, stream::FusedStream, Stream};
use solvent::prelude::{Packet, EPIPE};
use solvent_async::ipc::Channel;
use solvent_std::sync::Arsc;

use crate::Error;

pub struct Server {
    inner: Arsc<Inner>,
}

impl Server {
    pub fn new(channel: Channel) -> Self {
        Server {
            inner: Arsc::new(Inner {
                channel,
                stop: AtomicBool::new(false),
                has_stream: AtomicBool::new(false),
            }),
        }
    }

    pub fn packet_stream(&self) -> Option<PacketStream> {
        if self.inner.has_stream.swap(true, SeqCst) {
            return None;
        }
        Some(PacketStream {
            inner: self.inner.clone(),
        })
    }

    #[inline]
    pub fn send(&self, packet: Packet) -> Result<(), Error> {
        self.inner.send(packet)
    }
}

impl Drop for Server {
    #[inline]
    fn drop(&mut self) {
        self.inner.stop.store(true, Release);
    }
}

pub struct PacketStream {
    inner: Arsc<Inner>,
}

impl Stream for PacketStream {
    type Item = Result<Packet, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.inner.stop.load(Acquire) {
            return Poll::Ready(None);
        }

        let fut = self.inner.receive();
        pin_mut!(fut);
        let res = ready!(fut.poll(cx));
        Poll::Ready(match res {
            Err(Error::Disconnected) => None,
            res => Some(res),
        })
    }
}

impl FusedStream for PacketStream {
    #[inline]
    fn is_terminated(&self) -> bool {
        self.inner.stop.load(Acquire)
    }
}

struct Inner {
    channel: Channel,
    stop: AtomicBool,
    has_stream: AtomicBool,
}

impl Inner {
    async fn receive(&self) -> Result<Packet, Error> {
        let mut packet = Default::default();
        let res = self.channel.receive_packet(&mut packet).await;
        res.map_err(|err| {
            if err == EPIPE {
                self.stop.store(true, Release);
                Error::Disconnected
            } else {
                Error::ServerReceive(err)
            }
        })?;
        Ok(packet)
    }

    fn send(&self, mut packet: Packet) -> Result<(), Error> {
        let res = self.channel.send_packet(&mut packet);
        res.map_err(|err| {
            if err == EPIPE {
                self.stop.store(true, Release);
                Error::Disconnected
            } else {
                Error::ServerReceive(err)
            }
        })?;
        Ok(())
    }
}
