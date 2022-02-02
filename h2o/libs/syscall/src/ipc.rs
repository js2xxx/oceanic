#[cfg(feature = "call")]
// #[cfg(trace_assertions)]
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

pub const MAX_HANDLE_COUNT: usize = 256;
pub const MAX_BUFFER_SIZE: usize = crate::mem::PAGE_SIZE;
pub const CUSTOM_MSG_ID_START: usize = 0;
pub const CUSTOM_MSG_ID_END: usize = 12;

pub const SIG_GENERIC: usize = 0b001;
pub const SIG_READ: usize = 0b010;
pub const SIG_WRITE: usize = 0b100;
