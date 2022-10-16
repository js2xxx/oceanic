use alloc::string::String;

#[cfg(feature = "runtime")]
use entry::EntryServer;
use solvent::ipc::Channel;
#[cfg(feature = "std")]
use solvent_core::path::PathBuf;
#[cfg(feature = "std")]
use solvent::obj::Handle;

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

    fn event_token() -> Result<Handle, Error>;

    fn rename(src: String, dst_parent: Handle, dst: String) -> Result<(), Error>;

    fn link(src: String, dst_parent: Handle, dst: String) -> Result<(), Error>;

    fn unlink(name: String) -> Result<(), Error>;
}
