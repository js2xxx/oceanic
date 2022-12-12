use alloc::collections::BTreeMap;
use core::{
    iter::FusedIterator,
    mem,
    num::NonZeroUsize,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::*},
    time::Duration,
};

use crossbeam::queue::SegQueue;
use solvent::{
    error::{ENOENT, EPIPE, ETIME},
    ipc::{Channel, Packet, SIG_READ},
    prelude::Object,
    time::Instant,
};
use solvent_async::disp::DispSender;
use solvent_core::sync::{Arsc, Mutex};

use crate::Error;

#[derive(Debug, Clone)]
pub struct ClientImpl {
    inner: Arsc<Inner>,
}

impl ClientImpl {
    pub fn new(channel: Channel) -> Self {
        ClientImpl {
            inner: Arsc::new(Inner {
                next_id: AtomicUsize::new(1),
                channel,
                events: SegQueue::new(),
                callers: Mutex::new(BTreeMap::new()),
                set_event_receiver: AtomicBool::new(false),
                stop: AtomicBool::new(false),
            }),
        }
    }

    fn into_async_with_disp(self, disp: DispSender) -> Result<crate::ClientImpl, Self> {
        let channel = Channel::try_from(self)?;
        let channel = solvent_async::ipc::Channel::with_disp(channel, disp);
        Ok(crate::ClientImpl::from(channel))
    }

    #[inline]
    pub fn call(&self, packet: Packet) -> Result<Packet, Error> {
        self.inner.call(packet)
    }

    #[inline]
    pub fn call_timeout(&self, packet: Packet, timeout: Duration) -> Result<Packet, Error> {
        self.inner.call_timeout(packet, timeout)
    }

    #[inline]
    pub fn event_receiver(&self, timeout: Option<Duration>) -> Option<EventReceiverImpl> {
        (!self.inner.set_event_receiver.swap(true, SeqCst)).then(|| EventReceiverImpl {
            inner: self.inner.clone(),
            timeout,
        })
    }
}

impl AsRef<Channel> for ClientImpl {
    #[inline]
    fn as_ref(&self) -> &Channel {
        &self.inner.channel
    }
}

impl From<Channel> for ClientImpl {
    #[inline]
    fn from(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl TryFrom<ClientImpl> for Channel {
    type Error = ClientImpl;

    fn try_from(client: ClientImpl) -> Result<Self, Self::Error> {
        match Arsc::try_unwrap(client.inner) {
            Ok(mut inner) => {
                if inner.callers.get_mut().is_empty() {
                    Ok(inner.channel)
                } else {
                    Err(ClientImpl {
                        inner: Arsc::new(inner),
                    })
                }
            }
            Err(inner) => Err(ClientImpl { inner }),
        }
    }
}

pub struct EventReceiverImpl {
    inner: Arsc<Inner>,
    timeout: Option<Duration>,
}

impl Iterator for EventReceiverImpl {
    type Item = Result<Packet, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.inner.stop.load(Acquire) {
            return None;
        }

        let res = self.timeout.map_or_else(
            || self.inner.receive_event(),
            |timeout| self.inner.receive_event_timeout(timeout),
        );
        match res {
            Err(Error::Disconnected) => None,
            res => Some(res),
        }
    }
}

impl FusedIterator for EventReceiverImpl {}

#[derive(Debug)]
struct Inner {
    next_id: AtomicUsize,
    channel: Channel,
    events: SegQueue<Packet>,
    callers: Mutex<BTreeMap<usize, Packet>>,
    set_event_receiver: AtomicBool,
    stop: AtomicBool,
}

impl Inner {
    fn call(&self, packet: Packet) -> Result<Packet, Error> {
        self.call_inner(packet, |_| {
            self.channel
                .try_wait(Duration::MAX, true, false, SIG_READ)
                .map_err(Error::ClientReceive)?;
            Ok(())
        })
    }

    fn call_timeout(&self, packet: Packet, timeout: Duration) -> Result<Packet, Error> {
        self.call_inner(packet, |instant| {
            let elapsed = instant.elapsed();
            if elapsed >= timeout {
                return Err(Error::ClientReceive(ETIME));
            }
            self.channel
                .try_wait(timeout - elapsed, true, false, SIG_READ)
                .map_err(Error::ClientReceive)?;
            Ok(())
        })
    }

    #[inline]
    fn call_inner<F>(&self, mut packet: Packet, mut wait: F) -> Result<Packet, Error>
    where
        F: FnMut(Instant) -> Result<(), Error>,
    {
        let self_id = self.next_id.fetch_add(1, SeqCst);
        packet.id = NonZeroUsize::new(self_id);
        self.channel.send(&mut packet).map_err(|err| {
            if err == EPIPE {
                self.stop.store(true, Release);
                Error::Disconnected
            } else {
                Error::ClientReceive(err)
            }
        })?;

        let instant = Instant::now();
        loop {
            match self.channel.receive(&mut packet) {
                Ok(()) => {
                    if let Some(id) = packet.id {
                        if id.get() == self_id {
                            break Ok(packet);
                        } else {
                            let mut callers = self.callers.lock();
                            callers.insert(id.get(), mem::take(&mut packet));
                        }
                    } else {
                        self.events.push(mem::take(&mut packet));
                    }
                }
                Err(ENOENT) => {
                    let mut callers = self.callers.lock();
                    if let Some(packet) = callers.remove(&self_id) {
                        break Ok(packet);
                    }
                    wait(instant)?;
                }
                Err(err) => {
                    if err == EPIPE {
                        self.stop.store(true, Release);
                        break Err(Error::Disconnected);
                    }
                    break Err(Error::ClientReceive(err));
                }
            }
        }
    }

    fn receive_event(&self) -> Result<Packet, Error> {
        self.receive_event_inner(|_| {
            self.channel
                .try_wait(Duration::MAX, true, false, SIG_READ)
                .map_err(Error::ClientReceive)?;
            Ok(())
        })
    }

    fn receive_event_timeout(&self, timeout: Duration) -> Result<Packet, Error> {
        self.receive_event_inner(|instant| {
            let elapsed = instant.elapsed();
            if elapsed >= timeout {
                return Err(Error::ClientReceive(ETIME));
            }
            self.channel
                .try_wait(timeout - elapsed, true, false, SIG_READ)
                .map_err(Error::ClientReceive)?;
            Ok(())
        })
    }

    #[inline]
    fn receive_event_inner<F>(&self, mut wait: F) -> Result<Packet, Error>
    where
        F: FnMut(Instant) -> Result<(), Error>,
    {
        let instant = Instant::now();
        let mut packet = Default::default();
        loop {
            match self.channel.receive(&mut packet) {
                Ok(()) => {
                    if let Some(id) = packet.id {
                        let mut callers = self.callers.lock();
                        callers.insert(id.get(), mem::take(&mut packet));
                    } else {
                        break Ok(packet);
                    }
                }
                Err(ENOENT) => {
                    if let Some(packet) = self.events.pop() {
                        break Ok(packet);
                    }
                    wait(instant)?;
                }
                Err(err) => {
                    if err == EPIPE {
                        self.stop.store(true, Release);
                        break Err(Error::Disconnected);
                    }
                    break Err(Error::ClientReceive(err));
                }
            }
        }
    }
}

pub trait Client: From<Channel> + AsRef<Channel> {
    type EventReceiver: EventReceiver;
    type Async: crate::Client;

    fn from_inner(inner: ClientImpl) -> Self;

    fn into_inner(this: Self) -> ClientImpl;

    #[inline]
    fn into_async_with_disp(self, disp: DispSender) -> Result<Self::Async, Self> {
        Self::into_inner(self)
            .into_async_with_disp(disp)
            .map(<Self::Async as crate::Client>::from_inner)
            .map_err(Self::from_inner)
    }

    #[inline]
    #[cfg(feature = "runtime")]
    fn into_async(self) -> Result<Self::Async, Self> {
        self.into_async_with_disp(solvent_async::dispatch())
    }

    fn event_receiver(&self, timeout: Option<Duration>) -> Option<Self::EventReceiver>;
}

pub trait EventReceiver: FusedIterator<Item = Result<Self::Event, Error>> {
    type Event: crate::Event;
}

impl<T: FusedIterator<Item = Result<E, Error>>, E: crate::Event> EventReceiver for T {
    type Event = E;
}
