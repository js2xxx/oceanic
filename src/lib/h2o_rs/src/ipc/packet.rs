use alloc::vec::Vec;
use core::{fmt::Debug, num::NonZeroUsize};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Packet {
    pub id: Option<NonZeroUsize>,
    pub buffer: Vec<u8>,
    pub handles: Vec<sv_call::Handle>,
}

impl Packet {
    #[inline]
    pub fn clear(&mut self) {
        self.id = None;
        self.buffer.clear();
        self.handles.clear();
    }
}
