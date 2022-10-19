#[cfg(feature = "runtime")]
mod handle;
mod stream;

use alloc::boxed::Box;

use async_trait::async_trait;
use solvent::prelude::Phys;
use solvent_async::io::Stream;
use solvent_core::io::RawStream;
use solvent_rpc::io::{Error, file::PhysOptions};

#[cfg(feature = "runtime")]
pub use self::handle::{handle, handle_mapped};
pub use self::stream::FileStream;
use crate::entry::Entry;

#[async_trait]
pub trait File: Entry {
    async fn lock(&self, stream: Option<RawStream>) -> Result<Option<Stream>, Error>;

    /// # Errors
    ///
    /// This function will return an error if the unlock operation failed.
    ///
    /// # Safety
    ///
    /// The caller must ensure the file is locked before calling this function.
    unsafe fn unlock(&self) -> Result<(), Error>;

    async fn flush(&self) -> Result<(), Error>;

    async fn read_at(&self, pos: usize, buf: &mut [u8]) -> Result<usize, Error>;

    async fn write_at(&self, pos: usize, buf: &[u8]) -> Result<usize, Error>;

    async fn len(&self) -> Result<usize, Error>;

    #[inline]
    async fn is_empty(&self) -> Result<bool, Error> {
        self.len().await.map(|len| len != 0)
    }

    async fn resize(&self, new_len: usize) -> Result<(), Error>;

    async fn phys(&self, options: PhysOptions) -> Result<Phys, Error>;
}
