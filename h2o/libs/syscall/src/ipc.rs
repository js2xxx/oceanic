#[cfg(feature = "call")]
pub(crate) mod test;

use crate::Handle;

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct RawPacket {
    pub id: usize,
    pub handles: *mut Handle,
    pub handle_count: usize,
    pub handle_cap: usize,
    pub buffer: *mut u8,
    pub buffer_size: usize,
    pub buffer_cap: usize,
}

