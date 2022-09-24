mod thiserror; // Hacking `thiserror::Error`.
use core as std; // Hacking `thiserror::Error`.

use solvent::error::Error as RawError;
use thiserror_impl::Error;

#[derive(Error, Debug, PartialEq, Eq)]
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
}
