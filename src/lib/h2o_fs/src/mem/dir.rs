use alloc::{
    boxed::Box,
    collections::{btree_map::Entry as MapEntry, BTreeMap},
    string::String,
};

use async_trait::async_trait;
use solvent::prelude::Channel;
use solvent_core::{
    path::{Component, Path, PathBuf},
    sync::{Arsc, Mutex},
};
use solvent_rpc::io::{
    dir::{DirEntry, DirectoryServer},
    Error, FileType, Metadata, OpenOptions, Permission,
};

use crate::{
    dir::{handle, handle_mut, Directory, DirectoryMut, EventTokens},
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
        tokens: EventTokens,
        path: &Path,
        options: OpenOptions,
        conn: Channel,
    ) -> Result<bool, Error> {
        if options.intersects(OpenOptions::CREATE | OpenOptions::CREATE_NEW) {
            return Err(Error::PermissionDenied(Permission::WRITE));
        }
        match path.components().next() {
            Some(Component::Normal(name)) => {
                let name = name
                    .to_str()
                    .ok_or_else(|| Error::InvalidPath(path.into()))?;
                let path = path.strip_prefix(name).unwrap();
                let entry = self.get(name)?;
                entry.open(tokens, path, options, conn)
            }
            Some(_) => Err(Error::InvalidPath(path.into())),
            None => {
                let require = options.require();
                if !self.perm.contains(require) {
                    return Err(Error::PermissionDenied(require - self.perm));
                }
                let server = DirectoryServer::new(conn.into());
                let task = handle(self, tokens, server, options);
                solvent_async::spawn(task).detach();
                Ok(false)
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

pub trait FileInserter: Fn(&str) -> Result<Arsc<dyn Entry>, Error> + Send + Sync {}
impl<F: Fn(&str) -> Result<Arsc<dyn Entry>, Error> + Send + Sync> FileInserter for F {}

pub struct MemDirMut {
    entries: Mutex<BTreeMap<String, Arsc<dyn Entry>>>,
    perm: Permission,
    path: PathBuf,
    file_inserter: Arsc<dyn FileInserter>,
}

impl MemDirMut {
    pub fn new<F: FileInserter + 'static>(
        perm: Permission,
        path: PathBuf,
        file_inserter: Arsc<F>,
    ) -> Self {
        MemDirMut {
            entries: Mutex::new(BTreeMap::new()),
            perm,
            path,
            file_inserter,
        }
    }

    pub fn new_unsized(
        perm: Permission,
        path: PathBuf,
        file_inserter: Arsc<dyn FileInserter>,
    ) -> Self {
        MemDirMut {
            entries: Mutex::new(BTreeMap::new()),
            perm,
            path,
            file_inserter,
        }
    }

    fn get(&self, name: &str) -> Result<Arsc<dyn Entry>, Error> {
        if name.len() > MAX_NAME {
            return Err(Error::InvalidNameLength(name.len()));
        }
        match self.entries.lock().get(name) {
            Some(ent) => Ok(ent.clone()),
            None => Err(Error::NotFound),
        }
    }

    fn get_or_insert(
        &self,
        name: &str,
        options: OpenOptions,
        next: &Path,
    ) -> Result<(Arsc<dyn Entry>, bool), Error> {
        if name.len() > MAX_NAME {
            return Err(Error::InvalidNameLength(name.len()));
        }
        let mut entries = self.entries.lock();
        if let Some(ent) = entries.get(name) {
            if options.contains(OpenOptions::CREATE_NEW) {
                return Err(Error::Exists);
            }
            return Ok((ent.clone(), false));
        }

        let entry = if next != Path::new("") {
            (self.file_inserter)(name)? as Arsc<dyn Entry>
        } else {
            Arsc::new(Self::new_unsized(
                options.require(),
                self.path.join(name),
                self.file_inserter.clone(),
            )) as Arsc<dyn Entry>
        };
        entries.insert(name.into(), entry.clone());
        Ok((entry, true))
    }

    fn insert(&self, name: String, ent: Arsc<dyn Entry>) -> Result<(), Error> {
        let mut entries = self.entries.lock();
        match entries.entry(name) {
            MapEntry::Vacant(vacant) => {
                vacant.insert(ent);
                Ok(())
            }
            MapEntry::Occupied(_) => Err(Error::Exists),
        }
    }

    fn remove(&self, name: &str) -> Result<(String, Arsc<dyn Entry>), Error> {
        self.entries
            .lock()
            .remove_entry(name)
            .ok_or(Error::NotFound)
    }
}

impl Entry for MemDirMut {
    fn open(
        self: Arsc<Self>,
        tokens: EventTokens,
        path: &Path,
        options: OpenOptions,
        conn: Channel,
    ) -> Result<bool, Error> {
        match path.components().next() {
            Some(Component::Normal(name)) => {
                let name = name
                    .to_str()
                    .ok_or_else(|| Error::InvalidPath(path.into()))?;
                let path = path.strip_prefix(name).unwrap();
                let (entry, created) =
                    if !options.intersects(OpenOptions::CREATE | OpenOptions::CREATE_NEW) {
                        (self.get(name)?, false)
                    } else {
                        self.get_or_insert(name, options, path)?
                    };
                entry
                    .open(tokens, path, options, conn)
                    .map(|res| res | created)
            }
            Some(_) => Err(Error::InvalidPath(path.into())),
            None => {
                if options.contains(OpenOptions::CREATE_NEW) {
                    return Err(Error::Exists);
                }
                let require = options.require();
                if !self.perm.contains(require) {
                    return Err(Error::PermissionDenied(require - self.perm));
                }
                let server = DirectoryServer::new(conn.into());
                let task = handle_mut(self, tokens, server, options);
                solvent_async::spawn(task).detach();
                Ok(false)
            }
        }
    }

    fn metadata(&self) -> Result<Metadata, Error> {
        Ok(Metadata {
            file_type: FileType::Directory,
            perm: self.perm,
            len: 0,
        })
    }
}

#[async_trait]
impl Directory for MemDirMut {
    async fn next_dirent(&self, last: Option<String>) -> Result<DirEntry, Error> {
        let entries = self.entries.lock();
        let (name, entry) = match last {
            Some(last) => entries.range(last..).next(),
            None => entries.iter().next(),
        }
        .map(|(name, entry)| (name.clone(), entry.clone()))
        .ok_or(Error::IterEnd)?;
        drop(entries);
        let metadata = entry.metadata()?;
        Ok(DirEntry { name, metadata })
    }
}

#[async_trait]
impl DirectoryMut for MemDirMut {
    async fn rename(
        self: Arsc<Self>,
        src: &str,
        dst_parent: Arsc<dyn DirectoryMut>,
        dst: &str,
    ) -> Result<(), Error> {
        let dst_parent = dst_parent.into_any().downcast::<Self>().unwrap();

        // Renaming `path/to` to `path/to/inner` will create dead cycle references.
        let dst_full = dst_parent.path.join(dst);
        let src_full = self.path.join(src);
        if let Ok(next) = dst_full.strip_prefix(&src_full) {
            if next != Path::new("") {
                return Err(Error::IsAncestorOrEquals {
                    ancestor: src_full,
                    descendant: dst_full,
                });
            }
        }

        let (name, ent) = self.remove(src)?;

        let res = dst_parent.insert(dst.into(), ent.clone());
        res.inspect_err(|_| drop(self.insert(name, ent)))?;

        Ok(())
    }

    async fn link(
        self: Arsc<Self>,
        src: &str,
        dst_parent: Arsc<dyn DirectoryMut>,
        dst: &str,
    ) -> Result<(), Error> {
        let dst_parent = dst_parent.into_any().downcast::<Self>().unwrap();

        // Linking `path/to` to `path/to/inner` will create cycle references.
        let dst_full = dst_parent.path.join(dst);
        let src_full = self.path.join(src);
        if let Ok(next) = dst_full.strip_prefix(&src_full) {
            if next != Path::new("") {
                return Err(Error::IsAncestorOrEquals {
                    ancestor: src_full,
                    descendant: dst_full,
                });
            }
        }

        let ent = self.get(src)?;

        dst_parent.insert(dst.into(), ent)
    }

    async fn unlink(&self, name: &str) -> Result<(), Error> {
        self.remove(name).map(drop)
    }
}
