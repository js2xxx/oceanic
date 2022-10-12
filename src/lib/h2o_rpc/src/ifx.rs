use solvent::prelude::Packet;
use solvent_rpc_core::packet::{Deserializer, SerdePacket, Serializer};

#[cfg(feature = "std")]
pub trait Protocol {
    type Client: crate::Client;
    type Server: crate::Server;

    type SyncClient: crate::sync::Client;
    // type SyncServer: crate::sync::Server;

    fn with_disp(disp: solvent_async::disp::DispSender) -> (Self::Client, Self::Server) {
        let (tx, rx) = solvent::ipc::Channel::new();
        let (tx, rx) = (
            solvent_async::ipc::Channel::with_disp(tx, disp.clone()),
            solvent_async::ipc::Channel::with_disp(rx, disp),
        );
        (Self::Client::from(tx), Self::Server::from(rx))
    }

    #[inline]
    fn channel() -> (Self::Client, Self::Server) {
        Self::with_disp(solvent_async::dispatch())
    }

    fn sync_client_with_disp(
        disp: solvent_async::disp::DispSender,
    ) -> (Self::SyncClient, Self::Server) {
        let (tx, rx) = solvent::ipc::Channel::new();
        let rx = solvent_async::ipc::Channel::with_disp(rx, disp);
        (Self::SyncClient::from(tx), Self::Server::from(rx))
    }

    #[inline]
    fn sync_channel() -> (Self::SyncClient, Self::Server) {
        Self::sync_client_with_disp(solvent_async::dispatch())
    }
}

#[cfg(feature = "std")]
pub fn with_disp<P: Protocol>(disp: solvent_async::disp::DispSender) -> (P::Client, P::Server) {
    P::with_disp(disp)
}

#[cfg(feature = "std")]
pub fn channel<P: Protocol>() -> (P::Client, P::Server) {
    P::channel()
}

#[cfg(feature = "std")]
pub fn sync_client_with_disp<P: Protocol>(
    disp: solvent_async::disp::DispSender,
) -> (P::SyncClient, P::Server) {
    P::sync_client_with_disp(disp)
}

#[cfg(feature = "std")]
pub fn sync_channel<P: Protocol>() -> (P::SyncClient, P::Server) {
    P::sync_channel()
}

pub trait Event: Sized {
    fn deserialize(packet: Packet) -> Result<Self, crate::Error>;

    fn serialize(self) -> Result<Packet, crate::Error>;
}

impl<T: SerdePacket> Event for T {
    #[inline]
    fn deserialize(packet: Packet) -> Result<Self, crate::Error> {
        let mut de = Deserializer::new(&packet);
        SerdePacket::deserialize(&mut de)
    }

    fn serialize(self) -> Result<Packet, crate::Error> {
        let mut packet = Default::default();
        let mut ser = Serializer::new(&mut packet);
        SerdePacket::serialize(self, &mut ser)?;
        Ok(packet)
    }
}
