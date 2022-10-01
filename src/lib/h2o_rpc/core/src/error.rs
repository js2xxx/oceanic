#[doc(hidden)]
mod thiserror; // Hacking `thiserror::Error`.
use alloc::boxed::Box;
use core as std; // Hacking `thiserror::Error`.
use core::error::Error as Trait;

use solvent::error::Error as RawError;
use thiserror_impl::Error;

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

    #[error("buffer too short: expected at least {expected_at_least}, found {found}")]
    BufferTooShort {
        found: usize,
        expected_at_least: usize,
    },

    #[error("invalid type when parsing buffer: {0}")]
    TypeMismatch(#[source] Box<dyn Trait>),

    #[error("invalid magic number: {0}")]
    InvalidMagic(usize),

    #[error("invalid method: expected {expected}, found {found}")]
    InvalidMethod { expected: usize, found: usize },

    #[error("extra buffer sized {extra_buffer_len} and {extra_handle_count} handles found")]
    SizeMismatch {
        extra_buffer_len: usize,
        extra_handle_count: usize,
    },

    #[error("The endpoint to be serialized is already in use")]
    EndpointInUse,
}
