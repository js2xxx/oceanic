pub mod ctx;

#[cfg(feature = "call")]
pub mod test;

use crate::Handle;

pub const DEFAULT_STACK_SIZE: usize = 256 * 1024;

pub const PRIO_DEFAULT: u16 = 20;

pub const TASK_CTL_KILL: u32 = 1;
pub const TASK_CTL_SUSPEND: u32 = 2;
pub const TASK_CTL_DETACH: u32 = 3;

pub const TASK_DBG_READ_REG: u32 = 1;
pub const TASK_DBG_WRITE_REG: u32 = 2;
pub const TASK_DBG_READ_MEM: u32 = 3;
pub const TASK_DBG_WRITE_MEM: u32 = 4;
pub const TASK_DBG_EXCEP_HDL: u32 = 5;

pub const TASK_DBGADDR_GPR: usize = 0x1000;
pub const TASK_DBGADDR_FPU: usize = 0x2000;

#[derive(Debug)]
#[repr(C)]
pub struct CreateInfo {
    pub name: *mut u8,
    pub name_len: usize,
    pub stack_size: usize,
    pub init_chan: Handle,
    pub func: *mut u8,
    pub arg: *mut u8,
}

#[cfg(feature = "call")]
pub fn exit<T>(res: crate::Result<T>) -> !
where
    T: crate::SerdeReg,
{
    let retval = crate::Error::encode(res.map(|o| o.encode()));
    let _ = crate::call::task_exit(retval);
    unreachable!();
}
