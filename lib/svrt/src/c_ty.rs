use modular_bitfield::{bitfield, BitfieldSpecifier};

#[derive(Debug, Copy, Clone, Default)]
#[repr(C)]
pub struct StartupArgsHeader {
    pub signature: [u8; 4],
    pub handle_info_offset: usize,
    pub handle_count: usize,
    pub args_offset: usize,
    pub args_len: usize,
    pub envs_offset: usize,
    pub envs_len: usize,
}
pub const STARTUP_ARGS_HEADER_SIZE: usize = core::mem::size_of::<StartupArgsHeader>();
pub const PACKET_SIG_STARTUP_ARGS: [u8; 4] = [0xaa, 0xcf, 0x2b, 0x9d];

impl StartupArgsHeader {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let mut ret = StartupArgsHeader::default();
        let bytes: [u8; STARTUP_ARGS_HEADER_SIZE] = bytes.try_into().ok()?;
        unsafe {
            { bytes.as_ptr() }
                .copy_to_nonoverlapping(&mut ret as *mut _ as *mut _, STARTUP_ARGS_HEADER_SIZE)
        };
        Some(ret)
    }

    pub fn as_bytes(&self) -> &[u8] {
        let ptr = self as *const _ as *const _;
        unsafe { core::slice::from_raw_parts(ptr, STARTUP_ARGS_HEADER_SIZE) }
    }
}

#[derive(Debug, Copy, Clone, BitfieldSpecifier)]
#[repr(u16)]
#[bits = 16]
pub enum HandleType {
    None = 0,
    RootVirt,
    VdsoPhys,
    ProgramPhys,
    LoadRpc,
}

#[derive(Copy, Clone)]
#[bitfield]
#[repr(C)]
pub struct HandleInfo {
    pub handle_type: HandleType,
    pub additional: u16,
}

impl PartialEq for HandleInfo {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl Eq for HandleInfo {}

impl PartialOrd for HandleInfo {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.bytes.partial_cmp(&other.bytes)
    }
}

impl Ord for HandleInfo {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.bytes.cmp(&other.bytes)
    }
}
