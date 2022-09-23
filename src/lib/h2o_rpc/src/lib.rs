#![no_std]
#![feature(result_option_inspect)]

extern crate alloc;

mod imp;
pub mod sync;

pub use self::imp::{Client, EventReceiver};
