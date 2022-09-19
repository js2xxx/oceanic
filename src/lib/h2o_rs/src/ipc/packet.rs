use alloc::vec::Vec;
use core::{fmt::Debug, mem};

use sv_call::Error;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Packet {
    pub id: Option<usize>,
    pub buffer: Vec<u8>,
    pub handles: Vec<sv_call::Handle>,
}

pub trait PacketTyped: Sized {
    type TryFromError: Into<Error>;
    fn into_packet(self) -> Packet;

    fn try_from_packet(packet: &mut Packet) -> Result<Self, Self::TryFromError>;

    fn from_packet(mut packet: Packet) -> Self
    where
        Self::TryFromError: Debug,
    {
        Self::try_from_packet(&mut packet).expect("Failed to parse packet")
    }
}

impl PacketTyped for Packet {
    type TryFromError = Error;

    fn into_packet(self) -> Packet {
        self
    }

    fn try_from_packet(packet: &mut Packet) -> Result<Self, Self::TryFromError> {
        Ok(mem::take(packet))
    }
}
