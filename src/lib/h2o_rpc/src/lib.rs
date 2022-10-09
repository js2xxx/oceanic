#![no_std]
#![feature(error_in_core)]
#![feature(result_option_inspect)]
#![feature(type_alias_impl_trait)]

extern crate alloc;

#[cfg(feature = "std")]
mod client;
mod imp;
#[cfg(feature = "std")]
mod server;
#[cfg(feature = "std")]
pub mod sync;

pub use solvent_rpc_core::*;

pub use self::imp::*;
#[cfg(feature = "std")]
pub use self::{client::*, server::*};
