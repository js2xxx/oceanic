use alloc::vec::Vec;
use core as std;

use solvent::error::Error as RawError;
use solvent_rpc_core::SerdePacket;
#[cfg(feature = "std")]
use solvent_std::io::RawStream;
use thiserror_impl::Error;

use crate as solvent_rpc;
use crate::{thiserror, core::*};

#[derive(SerdePacket, Debug, Error)]
pub enum Error {
    #[error("directory entry not exist")]
    NotExist,

    #[error("`{0:?}` denied")]
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

#[derive(SerdePacket, Debug, Clone)]
pub struct Metadata {
    pub file_type: FileType,
    pub len: usize,
}

#[derive(SerdePacket, Debug, Copy, Clone, PartialEq, Eq)]
pub enum FileType {
    File,
    Directory,
    FileLink,
    DirLink,
}

#[derive(SerdePacket, Debug, Clone)]
pub struct DirEntry {
    pub name: Vec<u8>,
    pub metadata: Metadata,
}

#[protocol]
pub trait DirIter {
    fn next() -> Result<Result<DirEntry, Error>, ()>;
}

#[protocol]
pub trait Directory {
    fn metadata() -> Result<Metadata, Error>;

    fn iter() -> Result<DirIterClient, Error>;
}

#[protocol]
pub trait File: crate::core::Cloneable {
    fn metadata() -> Result<Metadata, Error>;

    fn stream() -> Result<RawStream, Error>;

    fn flush() -> Result<(), Error>;
}

#[protocol]
pub trait Fs {
    fn root() -> Result<DirectoryClient, Error>;

    fn open_dir(path: Vec<u8>) -> Result<DirectoryClient, Error>;

    fn open_file(path: Vec<u8>, options: OpenOptions) -> Result<FileClient, Error>;

    fn create_dir(path: Vec<u8>, all: bool) -> Result<(), Error>;

    fn remove_dir(path: Vec<u8>, all: bool) -> Result<(), Error>;

    fn remove_file(path: Vec<u8>) -> Result<(), Error>;

    fn rename(old: Vec<u8>, new: Vec<u8>) -> Result<(), Error>;

    fn set_perm(path: Vec<u8>, perm: Permission) -> Result<(), Error>;

    fn link(old: Vec<u8>, new: Vec<u8>) -> Result<(), Error>;

    fn unlink(path: Vec<u8>) -> Result<(), Error>;

    fn mount(path: Vec<u8>, provider: FsClient) -> Result<(), Error>;

    fn unmount(path: Vec<u8>) -> Result<(), Error>;
}
