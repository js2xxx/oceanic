use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, Ordering::*};

use async_trait::async_trait;
use solvent::prelude::{Channel, Phys};
use solvent_async::io::Stream;
use solvent_core::{io::RawStream, path::Path, sync::Arsc};
use solvent_rpc::io::{
    file::{FileServer, PhysOptions},
    Error, FileType, Metadata, OpenOptions, Permission,
};

use crate::{
    dir::EventTokens,
    entry::Entry,
    file::{handle_mapped, File},
};

pub struct MemFile {
    phys: Phys,
    perm: Permission,
    locked: AtomicBool,
}

impl MemFile {
    #[inline]
    pub fn new(phys: Phys, perm: Permission) -> Self {
        MemFile {
            phys,
            perm,
            locked: AtomicBool::new(false),
        }
    }
}

impl Entry for MemFile {
    fn open(
        self: Arsc<Self>,
        tokens: EventTokens,
        path: &Path,
        options: OpenOptions,
        conn: Channel,
    ) -> Result<bool, Error> {
        if path != Path::new("")
            || options.intersects(OpenOptions::EXPECT_DIR | OpenOptions::EXPECT_RPC)
        {
            return Err(Error::InvalidType(FileType::File));
        }
        if self.locked.load(Acquire) {
            return Err(Error::WouldBlock);
        }
        let require = options.require();
        if !self.perm.contains(require) {
            return Err(Error::PermissionDenied(require - self.perm));
        }
        let stream = RawStream {
            phys: self.phys.clone(),
            seeker: 0,
        };
        let server = FileServer::new(conn.into());
        let task = handle_mapped(
            self,
            tokens,
            unsafe { Stream::new(stream) },
            server,
            options,
        );
        solvent_async::spawn(task).detach();
        Ok(false)
    }

    #[inline]
    fn metadata(&self) -> Result<Metadata, Error> {
        Ok(Metadata {
            file_type: FileType::File,
            perm: self.perm,
            len: self.phys.len(),
        })
    }
}

#[async_trait]
impl File for MemFile {
    async fn lock(&self, stream: Option<RawStream>) -> Result<Option<Stream>, Error> {
        if self.locked.swap(true, AcqRel) {
            Err(Error::WouldBlock)
        } else {
            // SAFETY: The exclusiveness is ensured.
            Ok(stream.map(|raw| unsafe { Stream::new(raw) }))
        }
    }

    #[inline]
    unsafe fn unlock(&self) -> Result<(), Error> {
        self.locked.store(false, Release);
        Ok(())
    }

    #[inline]
    async fn flush(&self) -> Result<(), Error> {
        Ok(())
    }

    async fn read_at(&self, _: usize, _: &mut [u8]) -> Result<usize, Error> {
        unimplemented!("Default implementation in `StreamFile`")
    }

    async fn write_at(&self, _: usize, _: &[u8]) -> Result<usize, Error> {
        unimplemented!("Default implementation in `StreamFile`")
    }

    async fn len(&self) -> Result<usize, Error> {
        unimplemented!("Default implementation in `StreamFile`")
    }

    async fn resize(&self, _: usize) -> Result<(), Error> {
        unimplemented!("Default implementation in `StreamFile`")
    }

    async fn phys(&self, options: PhysOptions) -> Result<Phys, Error> {
        if self.locked.load(Acquire) {
            return Err(Error::WouldBlock);
        }
        let copy = options == PhysOptions::Copy;
        self.phys
            .create_sub(0, self.phys.len(), copy)
            .map_err(Error::Other)
    }
}
