use alloc::{boxed::Box, string::String};

use async_trait::async_trait;
use solvent_rpc::io::{dir::DirEntry, Error};

use crate::entry::Entry;

#[async_trait]
pub trait Directory: Entry {
    async fn next_dirent(&self, last: Option<String>) -> Result<DirEntry, Error>;
}

pub mod sync {
    use alloc::string::String;

    use solvent_rpc::io::{
        dir::{directory_sync, DirEntry},
        Error,
    };

    #[derive(Clone)]
    pub struct Remote(pub directory_sync::DirectoryClient);

    impl Remote {
        #[inline]
        pub fn iter(self) -> RemoteIter {
            RemoteIter {
                inner: self.0,
                last: None,
                stop: false,
            }
        }
    }

    #[derive(Clone)]
    pub struct RemoteIter {
        inner: directory_sync::DirectoryClient,
        last: Option<String>,
        stop: bool,
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
