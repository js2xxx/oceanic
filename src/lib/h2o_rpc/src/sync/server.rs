use core::{
    iter::FusedIterator,
    sync::atomic::{AtomicBool, Ordering::*},
    time::Duration,
};

use solvent::prelude::{Channel, Object, Packet, EPIPE, SIG_READ};
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

    pub fn packet_stream(&self, timeout: Duration) -> Option<PacketIter> {
        if self.inner.has_stream.swap(true, SeqCst) {
            return None;
        }
        Some(PacketIter {
            inner: self.inner.clone(),
            timeout,
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

pub struct PacketIter {
    inner: Arsc<Inner>,
    timeout: Duration,
}

impl Iterator for PacketIter {
    type Item = Result<Packet, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.inner.stop.load(Acquire) {
            return None;
        }

        match self.inner.receive(self.timeout) {
            Err(Error::Disconnected) => None,
            res => Some(res),
        }
    }
}

impl FusedIterator for PacketIter {}

struct Inner {
    channel: Channel,
    stop: AtomicBool,
    has_stream: AtomicBool,
}

impl Inner {
    fn receive(&self, timeout: Duration) -> Result<Packet, Error> {
        let mut packet = Default::default();
        let res = self.channel.try_wait(timeout, false, SIG_READ);
        let res = res.and_then(|_| self.channel.receive(&mut packet));
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
