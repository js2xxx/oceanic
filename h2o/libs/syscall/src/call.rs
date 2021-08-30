#[cfg(feature = "call")]
pub mod raw;
pub mod reg;

#[allow(unused_imports)]
use crate::{Arguments, SerdeReg};

solvent_gen::syscall_stub!(0 => pub(crate) fn get_time(ptr: *mut u128));
#[cfg(debug_assertions)]
solvent_gen::syscall_stub!(1 => pub(crate) fn log(args: *const ::log::Record));