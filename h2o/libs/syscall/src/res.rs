pub const RES_MEM: u32 = 0;
pub const RES_PIO: u32 = 1;
pub const RES_GSI: u32 = 2;

bitflags::bitflags! {
    pub struct IntrConfig: u32 {
        const ACTIVE_HIGH     = 0b01;
        const LEVEL_TRIGGERED = 0b10;
    }
}
