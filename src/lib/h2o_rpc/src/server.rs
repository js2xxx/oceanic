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

#[repr(transparent)]
pub struct Server {
    inner: Arsc<Inner>,
}

impl Server {
    pub fn new(channel: Channel) -> Self {
        Server {
            inner: Arsc::new(Inner {
                channel,
                stop: AtomicBool::new(false),
            }),
        }
    }

    #[inline]
    pub fn serve(self) -> (PacketStream, EventSender) {
        (
            PacketStream {
                inner: self.inner.clone(),
            },
            EventSender { inner: self.inner },
        )
    }
}

impl AsRef<Channel> for Server {
    #[inline]
    fn as_ref(&self) -> &Channel {
        &self.inner.channel
    }
}

impl From<Channel> for Server {
    #[inline]
    fn from(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl TryFrom<Server> for Channel {
    type Error = Server;

    fn try_from(server: Server) -> Result<Self, Self::Error> {
        match Arsc::try_unwrap(server.inner) {
            Ok(mut inner) => {
                if !*inner.stop.get_mut() {
                    Ok(inner.channel)
                } else {
                    Err(Server {
                        inner: Arsc::new(inner),
                    })
                }
            }
            Err(inner) => Err(Server { inner }),
        }
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
                responder: Responder(EventSender {
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

#[repr(transparent)]
pub struct EventSender {
    inner: Arsc<Inner>,
}

impl EventSender {
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
pub struct Responder(EventSender);

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
