#![no_std]
#![feature(error_in_core)]
#![feature(result_option_inspect)]

extern crate alloc;

mod error;
mod imp;
pub mod sync;

pub use self::{
    error::Error,
    imp::{Client, EventReceiver},
};
