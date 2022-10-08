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

#[cfg(feature = "std")]
pub fn with_disp(disp: solvent_async::disp::DispSender) -> (Client, Server) {
    let (tx, rx) = solvent::ipc::Channel::new();
    let (tx, rx) = (
        solvent_async::ipc::Channel::with_disp(tx, disp.clone()),
        solvent_async::ipc::Channel::with_disp(rx, disp),
    );
    (Client::new(rx), Server::new(tx))
}

#[cfg(feature = "std")]
#[inline]
pub fn channel() -> (Client, Server) {
    with_disp(solvent_async::dispatch())
}
