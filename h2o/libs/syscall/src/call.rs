#[cfg(feature = "call")]
pub mod raw;
pub mod reg;

#[allow(unused_imports)]
use crate::{Arguments, SerdeReg};

solvent_gen::syscall_stub!(0 => pub(crate) fn get_time(ptr: *mut u128));
#[cfg(debug_assertions)]
solvent_gen::syscall_stub!(1 => pub(crate) fn log(args: *const ::log::Record));
solvent_gen::syscall_stub!(2 => pub(crate) fn task_exit(retval: usize));
solvent_gen::syscall_stub!(3 => 
      pub(crate) fn task_fn(name: *mut u8, stack_size: usize, func: *mut u8, arg: *mut u8) -> usize);
solvent_gen::syscall_stub!(5 => pub(crate) fn task_join(hdl: usize) -> usize);
