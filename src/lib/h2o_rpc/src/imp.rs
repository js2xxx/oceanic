use solvent::prelude::Packet;

pub mod load;

pub enum DefaultEvent {
    Unknown(Packet),
}

impl DefaultEvent {
    #[inline]
    #[allow(dead_code)]
    fn deserialize(packet: Packet) -> Result<Self, crate::Error> {
        Ok(Self::Unknown(packet))
    }
}
