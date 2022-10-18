#![no_std]
#![feature(btree_drain_filter)]
#![feature(result_option_inspect)]

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use solvent::prelude::Channel;
use solvent_core::path::{Path, PathBuf};
use solvent_rpc::io::{
    dir::DirectorySyncClient, entry::EntrySyncClient, file::FileSyncClient, Error, Metadata,
    OpenOptions,
};

pub mod dir;
pub mod entry;
pub mod file;
pub mod fs;
#[cfg(feature = "runtime")]
pub mod mem;
#[cfg(feature = "runtime")]
pub mod rpc;

extern crate alloc;

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
    read(path)
        .and_then(|vec| String::from_utf8(vec).map_err(|err| Error::InvalidData(err.to_string())))
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
