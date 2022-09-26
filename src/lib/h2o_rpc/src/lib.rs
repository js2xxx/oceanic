#![no_std]
#![feature(result_option_inspect)]

extern crate alloc;

#[cfg(feature = "std")]
mod client;
#[cfg(feature = "std")]
mod server;
#[cfg(feature = "std")]
pub mod sync;

pub use solvent_rpc_core::*;

#[cfg(feature = "std")]
pub use self::{client::*, server::*};
