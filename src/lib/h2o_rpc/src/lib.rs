#![no_std]
#![feature(error_in_core)]
#![feature(result_option_inspect)]
#![feature(type_alias_impl_trait)]

extern crate alloc;

#[cfg(feature = "std")]
mod client;
mod ifx;
#[path ="../target/imp/mod.rs"]
#[rustfmt::skip]
#[allow(
    dead_code, 
    unused, 
    clippy::module_inception, 
    clippy::let_unit_value,
    clippy::match_single_binding,
)]
mod imp;
#[cfg(feature = "std")]
mod server;
#[cfg(feature = "std")]
pub mod sync;

pub use solvent_rpc_core::*;

#[cfg(feature = "std")]
pub use self::{client::*, server::*};
pub use self::{ifx::*, imp::*};
