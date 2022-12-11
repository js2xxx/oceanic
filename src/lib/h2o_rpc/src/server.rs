use core::{
    fmt,
    future::Future,
    mem::ManuallyDrop,
    num::NonZeroUsize,
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering::*},
    task::{ready, Context, Poll},
};

use futures::{pin_mut, stream::FusedStream, Stream};
use solvent::prelude::{Handle, Object, Packet, EPIPE};
use solvent_async::ipc::Channel;
use solvent_core::sync::Arsc;

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

impl TryFrom<ServerImpl> for solvent::ipc::Channel {
    type Error = ServerImpl;

    #[inline]
    fn try_from(value: ServerImpl) -> Result<Self, Self::Error> {
        Channel::try_from(value).map(Into::into)
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
                responder: Responder {
                    sender: EventSenderImpl {
                        inner: self.inner.clone(),
                    },
                    id: packet.id,
                },
                packet,
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
    pub fn as_raw(&self) -> Handle {
        // SAFETY: `solvent` marks unsafe use for `Object::from_raw`
        unsafe { self.inner.channel.as_ref().raw() }
    }

    #[inline]
    pub fn close(self) {
        self.inner.stop.store(true, Release);
    }
}

pub struct Responder {
    sender: EventSenderImpl,
    id: Option<NonZeroUsize>,
}

impl Responder {
    #[inline]
    pub fn send(self, mut packet: Packet, close: bool) -> Result<(), Error> {
        packet.id = self.id;
        let ret = self.sender.send(packet);
        if close {
            self.sender.close();
        }
        ret
    }

    #[inline]
    pub fn close(self) {
        self.sender.close()
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

pub trait Server: AsRef<Channel> + From<Channel> {
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

    /// Send an event from a raw handle.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the handle is the inner channel of this
    /// event sender.
    unsafe fn send_from_raw<T>(handle: Handle, event: T)
    where
        T: Into<Self::Event>,
    {
        // SAFETY: We don't take the ownership from the handle, and the handle is the
        // inner channel of this event sender.
        let channel = unsafe { ManuallyDrop::new(solvent::ipc::Channel::from_raw(handle)) };
        if let Ok(mut packet) = crate::Event::serialize(event.into()) {
            let _ = channel.send(&mut packet);
        }
    }

    fn close(self);
}
