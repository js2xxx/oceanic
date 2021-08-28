#![no_std]
#![feature(bool_to_option)]
#![feature(result_into_ok_or_err)]

pub mod error;

pub use error::*;

#[derive(Debug, Copy, Clone)]
pub struct Arguments {
      pub fn_num: usize,
      pub args: [usize; 5],
}
