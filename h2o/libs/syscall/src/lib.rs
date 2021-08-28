#![no_std]
#![feature(asm)]
#![feature(bool_to_option)]
#![feature(lang_items)]
#![feature(result_into_ok_or_err)]

pub mod error;
#[cfg(feature = "call")]
pub mod rxx;
#[cfg(feature = "call")]
pub mod call;

pub use error::*;

#[derive(Debug, Copy, Clone)]
pub struct Arguments {
      pub fn_num: usize,
      pub args: [usize; 5],
}
