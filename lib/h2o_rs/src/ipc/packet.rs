use alloc::vec::Vec;

#[derive(Debug, Default)]
pub struct Packet {
    pub id: Option<usize>,
    pub buffer: Vec<u8>,
    pub handles: Vec<sv_call::Handle>,
}

pub trait PacketTyped: Sized {
    fn into_packet(self) -> Packet;

    fn try_from_packet(packet: &mut Packet) -> Option<Self>;

    fn from_packet(packet: &mut Packet) -> Self {
        Self::try_from_packet(packet).expect("Failed to parse packet")
    }
}
