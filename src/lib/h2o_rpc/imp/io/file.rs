#[cfg(feature = "runtime")]
use entry::EntryServer;
use solvent::{
    ipc::{Channel, Packet},
    mem::Phys,
};

use super::*;

bitflags::bitflags! {
    #[derive(Default, SerdePacket)]
    pub struct EventFlags: u32 {
        const UNLOCK = 0b0000_0001;
    }
}

#[derive(SerdePacket, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PhysOptions {
    Shared = 0,
    Copy = 1,
}

#[protocol(EventFlags)]
pub trait File: entry::Entry {
    /// Lock the entire file excluively until the connection is closed.
    ///
    /// If the file supports memory-backed stream, the stream will be returned.
    fn lock() -> Result<Result<RawStream, ()>, Error>;

    /// Flush the cached content into the underlying file.
    fn flush() -> Result<(), Error>;

    fn read(len: usize) -> Result<Vec<u8>, Error>;

    fn write(buf: Vec<u8>) -> Result<usize, Error>;

    fn seek(pos: SeekFrom) -> Result<usize, Error>;

    fn read_at(offset: usize, len: usize) -> Result<Vec<u8>, Error>;

    fn write_at(offset: usize, buf: Vec<u8>) -> Result<usize, Error>;

    fn resize(new_len: usize) -> Result<(), Error>;

    fn phys(options: PhysOptions) -> Result<Phys, Error>;
}
