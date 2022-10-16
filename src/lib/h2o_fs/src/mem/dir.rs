use alloc::{boxed::Box, collections::BTreeMap, string::String};

use async_trait::async_trait;
use solvent::prelude::Channel;
use solvent_core::{
    path::{Component, Path},
    sync::Arsc,
};
use solvent_rpc::io::{
    dir::{DirEntry, DirectoryServer},
    Error, FileType, Metadata, OpenOptions, Permission,
};

use crate::{
    dir::{handle, Directory},
    entry::Entry,
};

const MAX_NAME: usize = u8::MAX as _;

pub struct MemDir {
    entries: BTreeMap<String, Arsc<dyn Entry>>,
    perm: Permission,
}

impl MemDir {
    fn get(&self, name: &str) -> Result<Arsc<dyn Entry>, Error> {
        if name.len() > MAX_NAME {
            return Err(Error::InvalidNameLength(name.len()));
        }
        match self.entries.get(name) {
            Some(ent) => Ok(ent.clone()),
            None => Err(Error::NotFound),
        }
    }
}

impl Entry for MemDir {
    fn open(
        self: Arsc<Self>,
        path: &Path,
        options: OpenOptions,
        conn: Channel,
    ) -> Result<(), Error> {
        let require = options.require();
        if !self.perm.contains(require) {
            return Err(Error::PermissionDenied(require - self.perm));
        }
        if options.intersects(OpenOptions::CREATE | OpenOptions::CREATE_NEW) {
            return Err(Error::PermissionDenied(Permission::WRITE));
        }
        match path.components().next() {
            Some(Component::Normal(name)) => {
                let name = name
                    .to_str()
                    .ok_or_else(|| Error::InvalidPath(path.into()))?;
                let entry = self.get(name)?;
                let path = path.strip_prefix(name).unwrap();
                entry.open(path, options, conn)
            }
            Some(_) => Err(Error::InvalidPath(path.into())),
            None => {
                let server = DirectoryServer::new(conn.into());
                let task = handle(self, server, options);
                solvent_async::spawn(task).detach();
                Ok(())
            }
        }
    }

    #[inline]
    fn metadata(&self) -> Result<Metadata, Error> {
        Ok(Metadata {
            file_type: FileType::Directory,
            perm: self.perm,
            len: 0,
        })
    }
}

#[async_trait]
impl Directory for MemDir {
    async fn next_dirent(&self, last: Option<String>) -> Result<DirEntry, Error> {
        let (name, entry) = match last {
            Some(last) => self.entries.range(last..).next(),
            None => self.entries.iter().next(),
        }
        .map(|(name, entry)| (name.clone(), entry.clone()))
        .ok_or(Error::IterEnd)?;
        let metadata = entry.metadata()?;
        Ok(DirEntry { name, metadata })
    }
}
