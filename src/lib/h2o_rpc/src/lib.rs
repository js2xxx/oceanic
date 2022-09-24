#![no_std]
#![feature(error_in_core)]
#![feature(result_option_inspect)]

extern crate alloc;

mod client;
mod error;
mod server;
pub mod sync;

pub use self::{
    client::{Client, EventReceiver},
    error::Error,
    server::{PacketStream, Server},
};
