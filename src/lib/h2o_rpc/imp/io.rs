pub mod dir;
pub mod entry;
pub mod file;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core as std;

use solvent::error::Error as RawError;
#[cfg(feature = "std")]
use solvent_core::{
    io::{RawStream, SeekFrom},
    path::PathBuf,
};
use solvent_rpc_core::SerdePacket;
use thiserror_impl::Error;

pub use self::entry::{FileType, Metadata};
use crate as solvent_rpc;
use crate::{core::*, thiserror};

#[derive(SerdePacket, Debug, Error)]
#[cfg(feature = "std")]
pub enum Error {
    #[error("file or directory not found")]
    NotFound,

    #[error("file or directory already exists")]
    Exists,

    #[error("at the end of the iterator")]
    IterEnd,

    #[error("the entry is busy (locked)")]
    WouldBlock,

    #[error("at a path within the local FS, use direct query instead")]
    LocalFs(PathBuf),

    #[error("found dissatisfactory {0:?}")]
    InvalidType(FileType),

    #[error("invalid seek")]
    InvalidSeek,

    #[error("invalid path: {0:?}")]
    InvalidPath(PathBuf),

    #[error("permission `{0:?}` denied")]
    PermissionDenied(Permission),

    #[error("RPC error: {0}")]
    RpcError(String),

    #[error("unknown error: {0}")]
    Other(#[source] RawError),
}

#[cfg(feature = "std")]
impl From<solvent_async::io::Error> for Error {
    fn from(value: solvent_async::io::Error) -> Self {
        match value {
            solvent_async::io::Error::InvalidSeek(..) => Self::InvalidSeek,
            solvent_async::io::Error::Other(err) => Self::Other(err),
        }
    }
}

#[cfg(feature = "std")]
impl From<solvent_rpc_core::Error> for Error {
    fn from(value: solvent_rpc_core::Error) -> Self {
        Error::RpcError(value.to_string())
    }
}

bitflags::bitflags! {
    #[derive(SerdePacket, Default)]
    pub struct Permission: u32 {
        const READ = 0b0001;
        const WRITE = 0b0010;
        const EXECUTE = 0b0100;
    }

    #[derive(SerdePacket)]
    pub struct OpenOptions: u32 {
        const READ = 0b0000_0001;
        const WRITE = 0b0000_0010;
        const APPEND = 0b0000_0100;
        const CREATE = 0b0000_1000;
        const CREATE_NEW = 0b0001_0000;
        const TRUNCATE = 0b0010_0000;
    }
}
