#[cfg(feature = "call")]
pub mod raw;
pub mod reg;

use solvent_gen::syscall_stub;

#[allow(unused_imports)]
use crate::{Arguments, SerdeReg};

syscall_stub!(0 => pub(crate) fn get_time(ptr: *mut u128));
// #[cfg(debug_assertions)]
syscall_stub!(1 => pub(crate) fn log(args: *const ::log::Record));

syscall_stub!(2 => pub(crate) fn task_exit(retval: usize));
syscall_stub!(3 =>
    pub(crate) fn task_fn(
        name: *mut u8,
        stack_size: usize,
        func: *mut u8,
        arg: *mut u8
    ) -> u32
);
syscall_stub!(5 => pub(crate) fn task_join(hdl: u32) -> usize);
syscall_stub!(6 => pub(crate) fn task_ctl(hdl: u32, op: u32, data: *mut u8));
syscall_stub!(7 => pub(crate) fn task_sleep(ms: u32));

syscall_stub!(8 =>
    pub(crate) fn alloc_pages(
        virt: *mut u8,
        phys: usize,
        size: usize,
        align: usize,
        flags: u32
    ) -> *mut u8
);
syscall_stub!(9 => pub(crate) fn dealloc_pages(ptr: *mut u8) -> usize);
syscall_stub!(10 => pub(crate) unsafe fn modify_pages(ptr: *mut u8, size: usize, flags: u32));

syscall_stub!(13 => pub(crate) fn wo_create() -> u32);
syscall_stub!(15 => pub(crate) fn wo_notify(hdl: u32, n: usize) -> usize);
