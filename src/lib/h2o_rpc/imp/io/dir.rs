use alloc::string::String;

#[cfg(feature = "runtime")]
use entry::EntryServer;
use solvent::ipc::Channel;
#[cfg(feature = "std")]
use solvent_core::path::PathBuf;

use super::*;

bitflags::bitflags! {
    #[derive(Default, SerdePacket)]
    pub struct EventFlags: u32 {
        const ADD = 0b0000_0001;
        const REMOVE = 0b0000_0010;
    }
}

#[derive(SerdePacket, Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub metadata: Metadata,
}

#[protocol(EventFlags)]
pub trait Directory: entry::Entry {
    fn next_dirent(last: Option<String>) -> Result<DirEntry, Error>;

    fn rename(old: PathBuf, new: PathBuf) -> Result<(), Error>;

    fn link(old: PathBuf, new: PathBuf) -> Result<(), Error>;

    fn unlink(path: PathBuf) -> Result<(), Error>;
}
