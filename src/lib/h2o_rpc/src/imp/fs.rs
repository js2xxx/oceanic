use alloc::vec::Vec;
use core as std;

use solvent::error::Error as RawError;
use solvent_rpc_core::SerdePacket;
#[cfg(feature = "std")]
use solvent_std::io::RawStream;
use thiserror_impl::Error;

use crate as solvent_rpc;
use crate::thiserror;

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
    pub file_type: Type,
    pub len: usize,
}

#[derive(SerdePacket, Debug, Copy, Clone, PartialEq, Eq)]
pub enum Type {
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

#[crate::protocol]
pub trait DirIter {
    #[id(0x173468cda3)]
    async fn next() -> Result<Result<DirEntry, Error>, ()>;
}

#[crate::protocol]
pub trait Directory {
    #[id(0x917346abf84)]
    async fn metadata() -> Result<Metadata, Error>;

    #[id(0xa9cfe848132)]
    async fn iter() -> Result<DirIterClient, Error>;
}

#[crate::protocol]
pub trait File {
    #[id(0x1fe929acb34)]
    async fn metadata() -> Result<Metadata, Error>;

    #[id(0x9af82bd847a)]
    async fn stream() -> Result<RawStream, Error>;

    #[id(0x275365af492)]
    async fn flush() -> Result<(), Error>;
}

#[crate::protocol]
pub trait Fs {
    #[id(0x10abc98373f)]
    async fn root() -> Result<DirectoryClient, Error>;

    #[id(0x38725fdee934)]
    async fn open_dir(path: Vec<u8>) -> Result<DirectoryClient, Error>;

    #[id(0xbf87349a382)]
    async fn open_file(path: Vec<u8>, options: OpenOptions) -> Result<FileClient, Error>;

    #[id(0x8378924a)]
    async fn create_dir(path: Vec<u8>, all: bool) -> Result<(), Error>;

    #[id(0x381739432)]
    async fn remove_dir(path: Vec<u8>, all: bool) -> Result<(), Error>;

    #[id(0x94897adde5)]
    async fn remove_file(path: Vec<u8>) -> Result<(), Error>;

    #[id(0xbcd948dcb2)]
    async fn rename(old: Vec<u8>, new: Vec<u8>) -> Result<(), Error>;

    #[id(0xa928df398b)]
    async fn set_perm(path: Vec<u8>, perm: Permission) -> Result<(), Error>;

    #[id(0x9faae3982)]
    async fn link(old: Vec<u8>, new: Vec<u8>) -> Result<(), Error>;

    #[id(0xcd389cd73)]
    async fn unlink(path: Vec<u8>) -> Result<(), Error>;

    #[id(0x48df387eab)]
    async fn mount(path: Vec<u8>, provider: FsClient) -> Result<(), Error>;

    #[id(0xaef847fcd2)]
    async fn unmount(path: Vec<u8>) -> Result<(), Error>;
}
