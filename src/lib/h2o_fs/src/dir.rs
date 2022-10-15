use alloc::{boxed::Box, string::String};

use async_trait::async_trait;
use solvent_core::path::Path;
use solvent_rpc::io::{dir::DirEntry, Error};

use crate::entry::Entry;

#[async_trait]
pub trait Directory: Entry {
    async fn next_dirent(&self, last: Option<String>) -> Result<DirEntry, Error>;
}

#[async_trait]
pub trait DirectoryMut: Directory {
    async fn rename(&self, old: &Path, new: &Path) -> Result<(), Error>;

    async fn link(&self, old: &Path, new: &Path) -> Result<(), Error>;

    async fn unlink(&self, path: &Path) -> Result<(), Error>;
}

pub mod sync {
    use alloc::string::String;

    use solvent_rpc::io::{
        dir::{directory_sync, DirEntry},
        Error,
    };

    #[derive(Clone)]
    pub struct RemoteIter {
        inner: directory_sync::DirectoryClient,
        last: Option<String>,
        stop: bool,
    }

    impl From<directory_sync::DirectoryClient> for RemoteIter {
        fn from(dir: directory_sync::DirectoryClient) -> Self {
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
