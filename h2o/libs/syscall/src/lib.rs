#![no_std]
#![feature(asm)]
#![feature(bool_to_option)]
#![feature(lang_items)]
#![feature(result_into_ok_or_err)]

pub mod call;
pub mod error;
#[cfg(feature = "call")]
pub mod rxx;
#[cfg(feature = "call")]
pub mod log;
pub mod time;

pub use call::reg::*;
pub use error::*;
pub use solvent_gen::*;

#[derive(Debug, Copy, Clone)]
pub struct Arguments {
      pub fn_num: usize,
      pub args: [usize; 5],
}

pub type SyscallWrapper = unsafe extern "C" fn(usize, usize, usize, usize, usize) -> usize;