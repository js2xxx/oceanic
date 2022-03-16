use bitflags::bitflags;

use crate::SerdeReg;

pub const RES_MEM: u32 = 0;
pub const RES_PIO: u32 = 1;
pub const RES_GSI: u32 = 2;

bitflags! {
    #[repr(transparent)]
    pub struct IntrConfig: u32 {
        const ACTIVE_HIGH     = 0b01;
        const LEVEL_TRIGGERED = 0b10;
    }
}

impl SerdeReg for IntrConfig {
    fn encode(self) -> usize {
        self.bits() as usize
    }

    fn decode(val: usize) -> Self {
        Self::from_bits_truncate(val as u32)
    }
}
