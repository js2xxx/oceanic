pub mod hdl;
#[cfg(feature = "call")]
pub mod raw;
pub mod reg;

use solvent_gen::syscall_stub;

#[allow(unused_imports)]
use crate::{Arguments, Handle, SerdeReg};

syscall_stub!(0 => pub(crate) fn get_time(ptr: *mut u128));
// #[cfg(debug_assertions)]
syscall_stub!(1 => pub(crate) fn log(args: *const ::log::Record));

syscall_stub!(2 => pub(crate) fn task_exit(retval: usize));
syscall_stub!(3 =>
    pub(crate) fn task_fn(
        name: *mut u8,
        name_len: usize,
        stack_size: usize,
        func: *mut u8,
        arg: *mut u8
    ) -> Handle
);
syscall_stub!(5 => pub(crate) fn task_join(hdl: Handle) -> usize);
syscall_stub!(6 => pub(crate) fn task_ctl(hdl: Handle, op: u32, data: *mut u8));
syscall_stub!(7 => pub(crate) fn task_sleep(ms: u32));

syscall_stub!(8 =>
    pub(crate) fn virt_alloc(
        virt: *mut *mut u8,
        phys: usize,
        size: usize,
        align: usize,
        flags: u32
    ) -> Handle
);
syscall_stub!(9 =>
    pub(crate) unsafe fn virt_modify(
        hdl: Handle,
        ptr: *mut u8,
        size: usize,
        flags: u32
    )
);
syscall_stub!(10 => pub(crate) fn mem_alloc(size: usize, align: usize, flags: u32) -> *mut u8);
syscall_stub!(11 => pub(crate) fn mem_dealloc(ptr: *mut u8));

syscall_stub!(13 => pub(crate) fn wo_create() -> Handle);
syscall_stub!(15 => pub(crate) fn wo_notify(hdl: Handle, n: usize) -> usize);

syscall_stub!(20 => pub(crate) fn object_drop(hdl: Handle));
