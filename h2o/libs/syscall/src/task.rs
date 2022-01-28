pub mod ctx;
pub mod excep;

#[cfg(feature = "call")]
#[cfg(debug_assertions)]
pub mod test;

use crate::Handle;

pub const DEFAULT_STACK_SIZE: usize = 256 * 1024;

pub const PRIO_DEFAULT: u16 = 20;

pub const TASK_CFLAGS_SUSPEND: u32 = 0b0000_0001;

pub const TASK_CTL_KILL: u32 = 1;
pub const TASK_CTL_SUSPEND: u32 = 2;

pub const TASK_DBG_READ_REG: u32 = 1;
pub const TASK_DBG_WRITE_REG: u32 = 2;
pub const TASK_DBG_READ_MEM: u32 = 3;
pub const TASK_DBG_WRITE_MEM: u32 = 4;
pub const TASK_DBG_EXCEP_HDL: u32 = 5;

pub const TASK_DBGADDR_GPR: usize = 0x1000;
pub const TASK_DBGADDR_FPU: usize = 0x2000;

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct ExecInfo {
    pub name: *const u8,
    pub name_len: usize,
    pub space: Handle,
    pub entry: *mut u8,
    pub stack: *mut u8,
    pub init_chan: Handle,
    pub arg: u64,
}
