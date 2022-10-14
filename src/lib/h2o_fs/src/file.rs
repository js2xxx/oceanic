use alloc::boxed::Box;
use core::ops::Deref;

use async_trait::async_trait;
use solvent::prelude::Channel;
use solvent_async::io::Stream;
use solvent_rpc::io::Error;

use crate::entry::Entry;

#[async_trait]
pub trait File: Entry {
    async fn lock(&self) -> Result<Option<FileStream>, Error>;

    async fn flush(&self) -> Result<(), Error>;

    async fn read_at(&self, pos: usize, buf: &mut [u8]) -> Result<usize, Error>;

    async fn write_at(&self, pos: usize, buf: &[u8]) -> Result<usize, Error>;

    async fn len(&self) -> Result<usize, Error>;

    #[inline]
    async fn is_empty(&self) -> Result<bool, Error> {
        self.len().await.map(|len| len != 0)
    }

    async fn resize(&self, new_len: usize) -> Result<usize, Error>;
}

pub struct FileStream {
    _conn: Channel,
    stream: Stream,
}

impl FileStream {
    /// # Safety
    ///
    /// `conn` must be the file client corresponding with the stream.
    #[inline]
    pub unsafe fn new(conn: Channel, stream: Stream) -> Self {
        FileStream {
            _conn: conn,
            stream,
        }
    }
}

impl Deref for FileStream {
    type Target = Stream;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}
