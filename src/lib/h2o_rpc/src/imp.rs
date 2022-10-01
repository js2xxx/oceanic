use solvent::prelude::Packet;

pub struct UnknownEvent(pub Packet);

impl Event for UnknownEvent {
    #[inline]
    fn deserialize(packet: Packet) -> Result<Self, crate::Error> {
        Ok(Self(packet))
    }

    #[inline]
    fn serialize(self) -> Result<Packet, crate::Error> {
        Ok(self.0)
    }
}

pub trait Event: Sized {
    fn deserialize(packet: Packet) -> Result<Self, crate::Error>;

    fn serialize(self) -> Result<Packet, crate::Error>;
}

pub mod loader;
