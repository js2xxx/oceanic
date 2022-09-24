#[doc(hidden)]
mod thiserror; // Hacking `thiserror::Error`.
use core as std; // Hacking `thiserror::Error`.

use alloc::boxed::Box;
use solvent::error::Error as RawError;
use thiserror_impl::Error;
use core::error::Error as Trait;

#[derive(Error, Debug)]
pub enum Error {
    #[error("inner channel disconnected")]
    Disconnected,

    #[error("unexpected error when the client receives packets: {0}")]
    ClientReceive(#[source] RawError),

    #[error("unexpected error when the client sends packets: {0}")]
    ClientSend(#[source] RawError),

    #[error("unexpected error when the server receives packets: {0}")]
    ServerReceive(#[source] RawError),

    #[error("unexpected error when the server sends packets: {0}")]
    ServerSend(#[source] RawError),

    #[error("buffer too short: expected {expected_at_least}, found {len}")]
    BufferTooShort {
        len: usize,
        expected_at_least: usize,
    },

    #[error("invalid type when parsing buffer: {0}")]
    TypeMismatch(#[source] Box<dyn Trait>)
}
