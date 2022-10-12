use super::*;

#[derive(SerdePacket, Debug, Clone)]
pub struct DirEntry {
    pub name: Vec<u8>,
    pub metadata: Metadata,
}

#[protocol]
pub trait DirIter {
    fn next() -> Result<DirEntry, Error>;
}

#[protocol]
pub trait Directory: entry::Entry {
    fn iter() -> Result<DirIterClient, Error>;

    fn open(path: Vec<u8>, options: OpenOptions, conn: entry::EntryServer) -> Result<(), Error>;

    fn rename(old: Vec<u8>, new: Vec<u8>) -> Result<(), Error>;

    fn link(old: Vec<u8>, new: Vec<u8>) -> Result<(), Error>;

    fn unlink(path: Vec<u8>) -> Result<(), Error>;

    fn mount(path: Vec<u8>, provider: FsClient) -> Result<(), Error>;

    fn unmount(path: Vec<u8>) -> Result<(), Error>;
}
