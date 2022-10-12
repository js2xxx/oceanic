#[cfg(feature = "std")]
use entry::EntryServer;
use solvent::ipc::Packet;

use super::*;

bitflags::bitflags! {
    #[derive(SerdePacket)]
    pub struct EventFlags: u32 {
        const READABLE = 0b001;
        const WRITEABLE = 0b010;
    }
}

#[protocol(EventFlags)]
pub trait File: entry::Entry {
    fn stream() -> Result<RawStream, Error>;

    fn flush() -> Result<(), Error>;

    fn read(len: usize) -> Result<Vec<u8>, Error>;

    fn write(buf: Vec<u8>) -> Result<usize, Error>;

    fn seek(pos: SeekFrom) -> Result<usize, Error>;

    fn read_at(offset: usize, len: usize) -> Result<Vec<u8>, Error>;

    fn write_at(offset: usize, buf: Vec<u8>) -> Result<usize, Error>;

    fn resize(new_len: usize) -> Result<(), Error>;
}
