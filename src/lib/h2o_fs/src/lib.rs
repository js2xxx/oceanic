#![no_std]
#![feature(btree_drain_filter)]
#![feature(extend_one)]
#![feature(result_option_inspect)]
#![feature(slice_ptr_get)]

pub mod dir;
pub mod entry;
pub mod file;
pub mod fs;
pub mod loader;
pub mod mem;
pub mod process;
pub mod rpc;
mod spawn;

extern crate alloc;

#[cfg(feature = "runtime")]
pub use spawn::spawner;
pub use spawn::{Runner, Spawner};

#[cfg(feature = "std-local")]
mod std_local {
    use alloc::{
        string::{String, ToString},
        vec::Vec,
    };

    use solvent::prelude::Channel;
    use solvent_core::path::{Path, PathBuf};
    use solvent_rpc::io::{
        dir::DirectorySyncClient, entry::EntrySyncClient, file::FileSyncClient, Error, FileType,
        Metadata, OpenOptions,
    };

    use crate::fs;

    #[inline]
    pub fn canonicalize<P: AsRef<Path>>(path: P) -> Result<PathBuf, Error> {
        fs::local().canonicalize(path)
    }

    pub fn open<P: AsRef<Path>>(path: P, options: OpenOptions) -> Result<FileSyncClient, Error> {
        let (t, conn) = Channel::new();
        fs::local().open(path, options | OpenOptions::EXPECT_FILE, conn)?;
        Ok(FileSyncClient::from(t))
    }

    pub fn open_dir<P: AsRef<Path>>(
        path: P,
        options: OpenOptions,
    ) -> Result<DirectorySyncClient, Error> {
        let (t, conn) = Channel::new();
        fs::local().open(path, options | OpenOptions::EXPECT_DIR, conn)?;
        Ok(DirectorySyncClient::from(t))
    }

    #[inline]
    pub fn open_rpc<P: AsRef<Path>>(path: P, conn: Channel) -> Result<(), Error> {
        fs::local().open(
            path,
            OpenOptions::READ | OpenOptions::WRITE | OpenOptions::EXPECT_RPC,
            conn,
        )
    }

    pub fn metadata<P: AsRef<Path>>(path: P) -> Result<Metadata, Error> {
        let (t, conn) = Channel::new();
        fs::local().open(path, OpenOptions::READ, conn)?;
        let client = EntrySyncClient::from(t);
        client.metadata()?
    }

    #[inline]
    pub fn read_dir<P: AsRef<Path>>(path: P) -> Result<fs::DirIter, Error> {
        fs::local().read_dir(path)
    }

    pub fn read<P: AsRef<Path>>(path: P) -> Result<Vec<u8>, Error> {
        let file = open(path, OpenOptions::READ)?;
        file.read(isize::MAX as usize)?
    }

    pub fn read_to_string<P: AsRef<Path>>(path: P) -> Result<String, Error> {
        read(path).and_then(|vec| {
            String::from_utf8(vec).map_err(|err| Error::InvalidData(err.to_string()))
        })
    }

    pub fn write<P: AsRef<Path>, B: AsRef<[u8]>>(path: P, buf: B) -> Result<(), Error> {
        const CAP: usize = 64;
        let file = open(
            path,
            OpenOptions::READ | OpenOptions::WRITE | OpenOptions::CREATE | OpenOptions::TRUNCATE,
        )?;
        for buf in buf.as_ref().chunks(CAP) {
            let mut written = 0;
            loop {
                let buf = Vec::from(&buf[written..]);
                written += file.write(buf)??;
            }
        }
        Ok(())
    }

    #[inline]
    pub fn rename<P1: AsRef<Path>, P2: AsRef<Path>>(src: P1, dst: P2) -> Result<(), Error> {
        fs::local().rename(src, dst)
    }

    #[inline]
    pub fn link<P1: AsRef<Path>, P2: AsRef<Path>>(src: P1, dst: P2) -> Result<(), Error> {
        fs::local().link(src, dst)
    }

    #[inline]
    pub fn unlink<P: AsRef<Path>>(path: P) -> Result<(), Error> {
        fs::local().unlink(path, false)
    }

    #[inline]
    pub fn remove_dir<P: AsRef<Path>>(path: P) -> Result<(), Error> {
        fs::local().unlink(path, true)
    }

    pub fn remove_dir_all<P: AsRef<Path>>(path: P) -> Result<(), Error> {
        fn inner(client: DirectorySyncClient) -> Result<(), Error> {
            let mut name = None;
            loop {
                let dirent = client.next_dirent(name)?;
                match dirent {
                    Err(Error::IterEnd) => break Ok(()),
                    Ok(dirent) => {
                        if dirent.metadata.file_type == FileType::Directory {
                            let (t, conn) = Channel::new();
                            client.open(
                                dirent.name.clone().into(),
                                OpenOptions::READ | OpenOptions::WRITE | OpenOptions::EXPECT_DIR,
                                conn,
                            )??;
                            inner(DirectorySyncClient::from(t))?;
                            client.unlink(dirent.name.clone(), true)??;
                        } else {
                            client.unlink(dirent.name.clone(), false)??;
                        }
                        name = Some(dirent.name);
                    }
                    Err(err) => break Err(err),
                }
            }
        }
        let path = path.as_ref();
        let metadata = metadata(path)?;
        if metadata.file_type == FileType::Directory {
            inner(open_dir(path, OpenOptions::READ | OpenOptions::WRITE)?)?;
            remove_dir(path)?;
        } else {
            unlink(path)?;
        }
        Ok(())
    }
}
#[cfg(feature = "std-local")]
pub use std_local::*;
