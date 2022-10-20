#![no_std]
#![feature(error_in_core)]
#![feature(result_option_inspect)]
#![feature(type_alias_impl_trait)]

extern crate alloc;

#[cfg(feature = "runtime")]
mod client;
mod ifx;
#[path ="../target/imp/mod.rs"]
#[rustfmt::skip]
#[allow(unused, clippy::all)]
mod imp;
#[cfg(feature = "runtime")]
mod server;
#[cfg(feature = "std")]
pub mod sync;

pub use solvent_rpc_core::*;

#[cfg(feature = "runtime")]
pub use self::{client::*, server::*};
pub use self::{ifx::*, imp::*};
