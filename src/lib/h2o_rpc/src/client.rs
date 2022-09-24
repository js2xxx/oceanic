use alloc::collections::{btree_map::Entry, BTreeMap};
use core::{
    future::Future,
    mem,
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
use solvent_std::sync::{Arsc, Mutex};

use crate::Error;

pub struct Client {
    inner: Arsc<Inner>,
}

impl Client {
    pub fn new(channel: Channel) -> Self {
        Client {
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

    pub fn event_receiver(&self) -> Option<EventReceiver> {
        {
            let mut entry = self.inner.event.waker.lock();
            if let EventEntry::Init = *entry {
                *entry = EventEntry::WillWait
            } else {
                return None;
            }
        }
        Some(EventReceiver {
            inner: self.inner.clone(),
            stop: false,
        })
    }

    pub async fn call(&self, mut packet: Packet) -> Result<Packet, Error> {
        let id = self.inner.register();
        packet.id = Some(id);

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

pub struct EventReceiver {
    inner: Arsc<Inner>,
    stop: bool,
}

impl Unpin for EventReceiver {}

impl Stream for EventReceiver {
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

impl FusedStream for EventReceiver {
    #[inline]
    fn is_terminated(&self) -> bool {
        self.stop
    }
}

impl Drop for EventReceiver {
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
            if let Entry::Occupied(mut entry) = wakers.entry(id) {
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
