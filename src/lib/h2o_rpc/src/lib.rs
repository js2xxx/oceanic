#![no_std]
#![feature(result_option_inspect)]

extern crate alloc;

mod client;
mod server;
pub mod sync;

pub use solvent_rpc_core::{packet, Error, SerdePacket};

pub use self::{
    client::{Client, EventReceiver},
    server::{PacketStream, Server},
};
