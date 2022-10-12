pub mod dir;
pub mod entry;
pub mod file;

use alloc::vec::Vec;
use core as std;

use solvent::error::Error as RawError;
use solvent_rpc_core::SerdePacket;
#[cfg(feature = "std")]
use solvent_core::io::{RawStream, SeekFrom};
use thiserror_impl::Error;

pub use self::entry::{FileType, Metadata};
use crate as solvent_rpc;
use crate::{core::*, thiserror};

#[derive(SerdePacket, Debug, Error)]
pub enum Error {
    #[error("file or directory not found")]
    NotFound,

    #[error("file or directory already exists")]
    Exists,

    #[error("at the end of the iterator")]
    IterEnd,

    #[error("invalid seek")]
    InvalidSeek,

    #[error("permission `{0:?}` denied")]
    PermissionDenied(Permission),

    #[error("unknown error: {0}")]
    Other(#[source] RawError),
}

bitflags::bitflags! {
    #[derive(SerdePacket)]
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

#[protocol]
pub trait Fs {
    fn root(conn: dir::DirectoryServer) -> Result<(), Error>;

    fn canonicalize(path: Vec<u8>) -> Result<Vec<u8>, Error>;
}
