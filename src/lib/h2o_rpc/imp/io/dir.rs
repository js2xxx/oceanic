use alloc::string::String;

#[cfg(feature = "runtime")]
use entry::EntryServer;
use solvent::ipc::Channel;
#[cfg(feature = "std")]
use solvent_core::path::PathBuf;

use super::*;

#[derive(SerdePacket, Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub metadata: Metadata,
}

#[protocol]
pub trait DirIter {
    fn next() -> Result<DirEntry, Error>;
}

#[protocol]
pub trait Directory: entry::Entry {
    fn iter(conn: Channel) -> Result<(), Error>;

    fn rename(old: PathBuf, new: PathBuf) -> Result<(), Error>;

    fn link(old: PathBuf, new: PathBuf) -> Result<(), Error>;

    fn unlink(path: PathBuf) -> Result<(), Error>;
}
