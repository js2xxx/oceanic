use core::{
    fmt,
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering::*},
    task::{ready, Context, Poll},
};

use futures::{pin_mut, stream::FusedStream, Stream};
use solvent::prelude::{Packet, EPIPE};
use solvent_async::ipc::Channel;
use solvent_core::sync::Arsc;
use solvent_rpc_core::packet::{self, SerdePacket};

use crate::Error;

#[derive(Debug)]
#[repr(transparent)]
pub struct ServerImpl {
    inner: Arsc<Inner>,
}

impl ServerImpl {
    pub fn new(channel: Channel) -> Self {
        ServerImpl {
            inner: Arsc::new(Inner {
                channel,
                stop: AtomicBool::new(false),
            }),
        }
    }

    #[inline]
    pub fn serve(self) -> (PacketStream, EventSenderImpl) {
        (
            PacketStream {
                inner: self.inner.clone(),
            },
            EventSenderImpl { inner: self.inner },
        )
    }
}

impl AsRef<Channel> for ServerImpl {
    #[inline]
    fn as_ref(&self) -> &Channel {
        &self.inner.channel
    }
}

impl From<Channel> for ServerImpl {
    #[inline]
    fn from(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl TryFrom<ServerImpl> for Channel {
    type Error = ServerImpl;

    fn try_from(server: ServerImpl) -> Result<Self, Self::Error> {
        match Arsc::try_unwrap(server.inner) {
            Ok(mut inner) => {
                if !*inner.stop.get_mut() {
                    Ok(inner.channel)
                } else {
                    Err(ServerImpl {
                        inner: Arsc::new(inner),
                    })
                }
            }
            Err(inner) => Err(ServerImpl { inner }),
        }
    }
}

impl SerdePacket for ServerImpl {
    fn serialize(self, ser: &mut packet::Serializer) -> Result<(), Error> {
        match Channel::try_from(self) {
            Ok(channel) => Channel::into_inner(channel).serialize(ser),
            Err(_) => Err(Error::EndpointInUse),
        }
    }

    fn deserialize(de: &mut packet::Deserializer) -> Result<Self, Error> {
        let channel = Channel::new(SerdePacket::deserialize(de)?);
        Ok(Self::new(channel))
    }
}

pub struct Request {
    pub packet: Packet,
    pub responder: Responder,
}

#[repr(transparent)]
pub struct PacketStream {
    inner: Arsc<Inner>,
}

impl Stream for PacketStream {
    type Item = Result<Request, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.inner.stop.load(Acquire) {
            return Poll::Ready(None);
        }

        let fut = self.inner.receive();
        pin_mut!(fut);
        let res = ready!(fut.poll(cx));
        Poll::Ready(match res {
            Err(Error::Disconnected) => None,
            res => Some(res.map(|packet| Request {
                packet,
                responder: Responder(EventSenderImpl {
                    inner: self.inner.clone(),
                }),
            })),
        })
    }
}

impl FusedStream for PacketStream {
    #[inline]
    fn is_terminated(&self) -> bool {
        self.inner.stop.load(Acquire)
    }
}

#[derive(Clone)]
#[repr(transparent)]
pub struct EventSenderImpl {
    inner: Arsc<Inner>,
}

impl EventSenderImpl {
    #[inline]
    pub fn send(&self, packet: Packet) -> Result<(), Error> {
        if self.inner.stop.load(Acquire) {
            return Err(Error::Disconnected);
        }
        self.inner.send(packet)
    }

    #[inline]
    pub fn close(self) {
        self.inner.stop.store(true, Release);
    }
}

#[repr(transparent)]
pub struct Responder(EventSenderImpl);

impl Responder {
    #[inline]
    pub fn send(self, packet: Packet, close: bool) -> Result<(), Error> {
        let ret = self.0.send(packet);
        if close {
            self.0.close();
        }
        ret
    }

    #[inline]
    pub fn close(self) {
        self.0.close()
    }
}

struct Inner {
    channel: Channel,
    stop: AtomicBool,
}

impl fmt::Debug for Inner {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Inner").field("stop", &self.stop).finish()
    }
}

impl Inner {
    async fn receive(&self) -> Result<Packet, Error> {
        let mut packet = Default::default();
        let res = self.channel.receive(&mut packet).await;
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
        let res = self.channel.send(&mut packet);
        res.map_err(|err| {
            if err == EPIPE {
                self.stop.store(true, Release);
                Error::Disconnected
            } else {
                Error::ServerSend(err)
            }
        })?;
        Ok(())
    }
}

pub trait Server: SerdePacket + AsRef<Channel> + From<Channel> {
    type RequestStream: FusedStream;
    type EventSender: EventSender;

    fn from_inner(inner: ServerImpl) -> Self;

    fn serve(self) -> (Self::RequestStream, Self::EventSender);
}

pub trait EventSender {
    type Event: crate::Event;

    fn send_event(&self, event: Self::Event) -> Result<(), Error>;

    #[inline]
    fn send<T>(&self, event: T) -> Result<(), Error>
    where
        T: Into<Self::Event>,
    {
        self.send_event(event.into())
    }

    fn close(self);
}
