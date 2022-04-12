use bitflags::bitflags;

use crate::SerdeReg;

bitflags! {
    /// Flags to describe a block of memory.
    #[repr(transparent)]
    pub struct Flags: u32 {
        const USER_ACCESS = 1;
        const READABLE    = 1 << 1;
        const WRITABLE    = 1 << 2;
        const EXECUTABLE  = 1 << 3;
        const UNCACHED    = 1 << 4;
    }
}

impl SerdeReg for Flags {
    fn encode(self) -> usize {
        self.bits() as usize
    }

    fn decode(val: usize) -> Self {
        Self::from_bits_truncate(val as u32)
    }
}

#[derive(Debug, Default)]
#[repr(C)]
pub struct MemInfo {
    pub all_available: usize,
    pub current_used: usize,
}

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 4096;

#[repr(C)]
pub struct VirtMapInfo {
    pub offset: usize,
    pub phys: crate::Handle,
    pub phys_offset: usize,
    pub len: usize,
    pub align: usize,
    pub flags: Flags,
}
