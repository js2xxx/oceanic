mod event;
#[cfg(feature = "runtime")]
mod handle;

use alloc::{boxed::Box, string::String};

use async_trait::async_trait;
use solvent_core::sync::Arsc;
use solvent_rpc::io::{dir::DirEntry, Error};

pub use self::event::*;
#[cfg(feature = "runtime")]
pub use self::handle::*;
use crate::entry::Entry;

#[async_trait]
pub trait Directory: Entry {
    async fn next_dirent(&self, last: Option<String>) -> Result<DirEntry, Error>;
}

#[async_trait]
pub trait DirectoryMut: Directory {
    async fn rename(
        self: Arsc<Self>,
        src: &str,
        dst_parent: Arsc<dyn DirectoryMut>,
        dst: &str,
    ) -> Result<(), Error>;

    async fn link(
        self: Arsc<Self>,
        src: &str,
        dst_parent: Arsc<dyn DirectoryMut>,
        dst: &str,
    ) -> Result<(), Error>;

    async fn unlink(&self, name: &str, expect_dir: bool) -> Result<(), Error>;
}

pub mod sync {
    use alloc::string::String;

    use solvent_rpc::io::{
        dir::{DirEntry, DirectorySyncClient},
        Error,
    };

    #[derive(Clone)]
    pub struct RemoteIter {
        inner: DirectorySyncClient,
        last: Option<String>,
        stop: bool,
    }

    impl From<DirectorySyncClient> for RemoteIter {
        fn from(dir: DirectorySyncClient) -> Self {
            RemoteIter {
                inner: dir,
                last: None,
                stop: false,
            }
        }
    }

    impl Iterator for RemoteIter {
        type Item = Result<DirEntry, Error>;

        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            if self.stop {
                return None;
            }
            match self.inner.next_dirent(self.last.take()) {
                Ok(Err(Error::IterEnd)) => None,
                Ok(Ok(item)) => {
                    self.last = Some(item.name.clone());
                    Some(Ok(item))
                }
                Ok(Err(err)) => Some(Err(err)),
                Err(err) => {
                    self.stop = true;
                    Some(Err(err.into()))
                }
            }
        }
    }
}
