pub const RES_MEM: u32 = 0;
pub const RES_PIO: u32 = 1;
pub const RES_INTR: u32 = 2;

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Msi {
    pub target_address: u32,
    pub target_data: u32,

    pub vec_start: u8,
    pub vec_len: u8,
    pub apic_id: u32,
}
