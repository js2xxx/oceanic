use core as std; // Hacking for `thiserror::Error`.

use solvent::{
    error::Error as RawError,
    prelude::{ENOENT, EPIPE, ETIME},
};
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum Error {
    #[error("inner channel disconnected")]
    Disconnected,

    #[error("no packet available")]
    WouldBlock,

    #[error("timeout while waiting for packet")]
    Timeout,

    #[error("unexpected error when receiving packets: {0}")]
    Receive(RawError),
}

impl From<RawError> for Error {
    fn from(err: RawError) -> Self {
        match err {
            EPIPE => Error::Disconnected,
            ENOENT => Error::WouldBlock,
            ETIME => Error::Timeout,
            _ => Error::Receive(err),
        }
    }
}
