use alloc::collections::{btree_map::Entry, BTreeMap};
use core::{
    fmt,
    future::Future,
    mem,
    num::NonZeroUsize,
    pin::Pin,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::*},
    task::{Context, Poll, Waker},
};

use crossbeam::queue::SegQueue;
use futures::{pin_mut, ready, stream::FusedStream, Stream};
use solvent::{
    error::{ENOENT, EPIPE},
    ipc::Packet,
};
use solvent_async::ipc::Channel;
use solvent_core::sync::{Arsc, Mutex};
use solvent_rpc_core::packet::{self, SerdePacket};

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
                event: Event {
                    waker: Mutex::new(EventEntry::Init),
                    packets: SegQueue::new(),
                },
                wakers: Mutex::new(BTreeMap::new()),
                stop: AtomicBool::new(false),
            }),
        }
    }

    pub fn into_sync(self) -> Result<crate::sync::ClientImpl, Self> {
        let channel = Channel::try_from(self)?;
        let channel = solvent::ipc::Channel::from(channel);
        Ok(crate::sync::ClientImpl::from(channel))
    }

    pub fn event_receiver(&self) -> Option<EventReceiverImpl> {
        {
            let mut entry = self.inner.event.waker.lock();
            if let EventEntry::Init = *entry {
                *entry = EventEntry::WillWait
            } else {
                return None;
            }
        }
        Some(EventReceiverImpl {
            inner: self.inner.clone(),
            stop: false,
        })
    }

    pub async fn call(&self, mut packet: Packet) -> Result<Packet, Error> {
        let id = self.inner.register();
        packet.id = NonZeroUsize::new(id);

        match self.inner.channel.send(&mut packet) {
            Err(EPIPE) => self.inner.receive_to_end().await?,
            res => res.map_err(Error::ClientSend)?,
        };

        Call {
            id,
            inner: Some(self.inner.clone()),
        }
        .await
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
                if inner.wakers.get_mut().is_empty() {
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

impl SerdePacket for ClientImpl {
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

pub struct EventReceiverImpl {
    inner: Arsc<Inner>,
    stop: bool,
}

impl Unpin for EventReceiverImpl {}

impl Stream for EventReceiverImpl {
    type Item = Result<Packet, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.stop {
            return Poll::Ready(None);
        }

        let res = {
            let res = self.inner.receive_for_event(cx.waker());
            pin_mut!(res);
            ready!(ready!(res.poll(cx)))
        };
        Poll::Ready(Some(res.inspect_err(|err| {
            if matches!(err, Error::Disconnected) {
                self.stop = true;
            }
        })))
    }
}

impl FusedStream for EventReceiverImpl {
    #[inline]
    fn is_terminated(&self) -> bool {
        self.stop
    }
}

impl Drop for EventReceiverImpl {
    fn drop(&mut self) {
        *self.inner.event.waker.lock() = EventEntry::Init
    }
}

struct Call {
    id: usize,
    inner: Option<Arsc<Inner>>,
}

impl Future for Call {
    type Output = Result<Packet, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let res = {
            let client = self.inner.as_ref().ok_or(Error::Disconnected)?;
            let res = client.receive_for_caller(self.id, cx.waker());
            pin_mut!(res);
            ready!(ready!(res.poll(cx)))
        };
        Poll::Ready(res.inspect(|_| {
            self.inner.take().expect("Polled after completion");
        }))
    }
}

impl Drop for Call {
    fn drop(&mut self) {
        if let Some(client) = self.inner.take() {
            client.deregister(self.id);
        }
    }
}

#[derive(Debug)]
struct Event {
    waker: Mutex<EventEntry>,
    packets: SegQueue<Packet>,
}

impl Event {
    fn receive(&self, packet: Packet) {
        self.packets.push(packet);
        self.waker.lock().wake()
    }
}

#[derive(Debug)]
enum EventEntry {
    Init,
    WillWait,
    Waiting(Waker),
}

impl EventEntry {
    fn wake(&mut self) {
        if let EventEntry::Waiting(waker) = self {
            waker.wake_by_ref();
            *self = EventEntry::WillWait;
        }
    }
}

impl Drop for EventEntry {
    #[inline]
    fn drop(&mut self) {
        self.wake()
    }
}

#[derive(Debug)]
enum WakerEntry {
    Init,
    Waiting(Waker),
    Packet(Packet),
    Fini,
}

impl Default for WakerEntry {
    #[inline]
    fn default() -> Self {
        WakerEntry::Init
    }
}

impl WakerEntry {
    fn register(&mut self, waker: &Waker) {
        match self {
            WakerEntry::Packet(_) => {}
            WakerEntry::Fini => unreachable!("Can't register finalized `WakerEntry`"),
            _ => *self = WakerEntry::Waiting(waker.clone()),
        }
    }

    fn deregister(&mut self) -> bool {
        match self {
            WakerEntry::Packet(_) => true,
            _ => {
                *self = WakerEntry::Fini;
                false
            }
        }
    }

    fn receive(&mut self, packet: Packet) -> bool {
        if let WakerEntry::Fini = self {
            return false;
        }

        if let Self::Waiting(waker) = self {
            waker.wake_by_ref();
            *self = WakerEntry::Packet(packet);
        }

        true
    }

    fn take(&mut self) -> Option<Packet> {
        match self {
            WakerEntry::Packet(packet) => {
                let packet = mem::take(packet);
                *self = WakerEntry::Fini;
                Some(packet)
            }
            _ => None,
        }
    }
}

impl Drop for WakerEntry {
    fn drop(&mut self) {
        if let Self::Waiting(waker) = self {
            waker.wake_by_ref();
        }
    }
}

struct Inner {
    next_id: AtomicUsize,
    channel: Channel,
    event: Event,
    wakers: Mutex<BTreeMap<usize, WakerEntry>>,
    stop: AtomicBool,
}

impl fmt::Debug for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Inner")
            .field("next_id", &self.next_id)
            .field("event", &self.event)
            .field("wakers", &self.wakers)
            .field("stop", &self.stop)
            .finish()
    }
}

impl Inner {
    #[inline]
    fn register(&self) -> usize {
        let id = self.next_id.fetch_add(1, SeqCst);
        self.wakers.lock().insert(id, Default::default());
        id
    }

    fn deregister(&self, id: usize) {
        let mut wakers = self.wakers.lock();
        let entry = wakers
            .get_mut(&id)
            .expect("Deregistering discarded `WakerEntry`");
        if entry.deregister() {
            wakers.remove(&id);
        }
    }

    async fn receive(&self) -> Result<(), Error> {
        let mut packet = Default::default();
        let res = self.channel.receive(&mut packet).await;
        res.map_err(|err| {
            if matches!(err, EPIPE) {
                self.stop.store(true, Release);
                Error::Disconnected
            } else {
                Error::ClientReceive(err)
            }
        })?;
        if let Some(id) = packet.id {
            let mut wakers = self.wakers.lock();
            if let Entry::Occupied(mut entry) = wakers.entry(id.get()) {
                if !entry.get_mut().receive(packet) {
                    entry.remove();
                }
            }
        } else {
            self.event.receive(packet)
        }
        Ok(())
    }

    async fn receive_to_end(&self) -> Result<(), Error> {
        loop {
            if self.stop.load(Acquire) {
                break Err(Error::Disconnected);
            }

            match self.receive().await {
                Ok(()) => {}
                Err(Error::ClientReceive(ENOENT)) => break Ok(()),
                Err(err) => break Err(err),
            }
        }
    }

    async fn receive_for_caller(&self, id: usize, waker: &Waker) -> Poll<Result<Packet, Error>> {
        {
            let mut wakers = self.wakers.lock();
            let entry = wakers.get_mut(&id).expect("Polling unregistered id");
            entry.register(waker);
        }

        let stop = match self.receive_to_end().await {
            Err(Error::Disconnected) => true,
            res => {
                res?;
                false
            }
        };

        let mut wakers = self.wakers.lock();
        let entry = wakers.get_mut(&id).expect("Polling unregistered id");
        if let Some(packet) = entry.take() {
            wakers.remove(&id);
            Poll::Ready(Ok(packet))
        } else if stop {
            Poll::Ready(Err(Error::Disconnected))
        } else {
            Poll::Pending
        }
    }

    async fn receive_for_event(&self, waker: &Waker) -> Poll<Result<Packet, Error>> {
        {
            let mut entry = self.event.waker.lock();
            *entry = EventEntry::Waiting(waker.clone());
        }

        let stop = match self.receive_to_end().await {
            Err(Error::Disconnected) => true,
            res => {
                res?;
                false
            }
        };

        if let Some(packet) = self.event.packets.pop() {
            Poll::Ready(Ok(packet))
        } else if stop {
            Poll::Ready(Err(Error::Disconnected))
        } else {
            Poll::Pending
        }
    }
}

pub trait Client: SerdePacket + From<Channel> + AsRef<Channel> {
    type EventReceiver: EventReceiver;
    type Sync: crate::sync::Client;

    fn from_inner(inner: ClientImpl) -> Self;

    fn into_inner(this: Self) -> ClientImpl;

    #[inline]
    fn into_sync(self) -> Result<Self::Sync, Self> {
        Self::into_inner(self)
            .into_sync()
            .map(<Self::Sync as crate::sync::Client>::from_inner)
            .map_err(Self::from_inner)
    }

    fn event_receiver(&self) -> Option<Self::EventReceiver>;
}

pub trait EventReceiver: FusedStream<Item = Result<Self::Event, Error>> {
    type Event: crate::Event;
}

impl<T: FusedStream<Item = Result<E, Error>>, E: crate::Event> EventReceiver for T {
    type Event = E;
}
