use alloc::{
    collections::{btree_map::Entry as MapEntry, BTreeMap},
    string::String,
};

use solvent_core::{
    path::{Path, PathBuf},
    sync::{Arsc, Mutex},
};
use solvent_rpc::io::{Error, Permission};

use super::{FileInserter, MemDir, MemDirMut};
use crate::entry::Entry;

#[derive(Default)]
pub struct Builder {
    entries: BTreeMap<String, BuilderInner>,
    perm: Permission,
}

enum BuilderInner {
    Dir(Builder),
    Entry(Arsc<dyn Entry>),
}

impl Builder {
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    pub fn entry(
        &mut self,
        path: &Path,
        perm: Permission,
        entry: Arsc<dyn Entry>,
    ) -> Result<&mut Self, Error> {
        let mut comps = path.components().peekable();
        let comp = comps
            .next()
            .ok_or_else(|| Error::InvalidPath(path.into()))?;
        let comp = comp
            .as_os_str()
            .to_str()
            .ok_or_else(|| Error::InvalidPath(path.into()))?;
        if comps.peek().is_none() {
            let file = BuilderInner::Entry(entry);
            match self.entries.entry(comp.into()) {
                MapEntry::Vacant(entry) => {
                    self.perm |= perm;
                    entry.insert(file);
                }
                MapEntry::Occupied(_) => return Err(Error::Exists),
            }
        } else {
            let mut map_ent;
            let dir = match self.entries.entry(comp.into()) {
                MapEntry::Vacant(entry) => entry.insert(BuilderInner::Dir(Builder::new())),
                MapEntry::Occupied(ent) => {
                    map_ent = ent;
                    map_ent.get_mut()
                }
            };
            let dir = match dir {
                BuilderInner::Dir(dir) => dir,
                BuilderInner::Entry(_) => return Err(Error::LocalFs(comp.into())),
            };
            self.perm |= perm;
            let path = PathBuf::from_iter(comps);
            dir.entry(&path, perm, entry)?;
        }
        Ok(self)
    }

    pub fn empty_path(&mut self, path: &Path, perm: Permission) -> Result<&mut Self, Error> {
        let mut comps = path.components().peekable();
        let comp = comps
            .next()
            .ok_or_else(|| Error::InvalidPath(path.into()))?;
        let comp = comp
            .as_os_str()
            .to_str()
            .ok_or_else(|| Error::InvalidPath(path.into()))?;

        let mut map_ent;
        let dir = match self.entries.entry(comp.into()) {
            MapEntry::Vacant(entry) => entry.insert(BuilderInner::Dir(Builder::new())),
            MapEntry::Occupied(ent) => {
                map_ent = ent;
                map_ent.get_mut()
            }
        };
        let dir = match dir {
            BuilderInner::Dir(dir) => dir,
            BuilderInner::Entry(_) => return Err(Error::LocalFs(comp.into())),
        };
        self.perm |= perm;
        if comps.peek().is_some() {
            let path = PathBuf::from_iter(comps);
            dir.empty_path(&path, perm)?;
        }
        Ok(self)
    }

    pub fn build(self) -> Arsc<MemDir> {
        let entries = self.entries.into_iter().map(|(name, entry)| match entry {
            BuilderInner::Dir(builder) => (name, builder.build() as Arsc<dyn Entry>),
            BuilderInner::Entry(entry) => (name, entry),
        });
        Arsc::new(MemDir {
            entries: entries.collect(),
            perm: self.perm,
        })
    }

    #[inline]
    pub fn build_mut<F: FileInserter + 'static>(self, file_inserter: Arsc<F>) -> Arsc<MemDirMut> {
        self.build_mut_inner("".into(), file_inserter)
    }

    fn build_mut_inner<F: FileInserter + 'static>(
        self,
        path: PathBuf,
        file_inserter: Arsc<F>,
    ) -> Arsc<MemDirMut> {
        let entries = self.entries.into_iter().map(|(name, entry)| match entry {
            BuilderInner::Dir(builder) => {
                let path = path.join(&name);
                (
                    name,
                    builder.build_mut_inner(path, file_inserter.clone()) as Arsc<dyn Entry>,
                )
            }
            BuilderInner::Entry(entry) => (name, entry),
        });
        Arsc::new(MemDirMut {
            entries: Mutex::new(entries.collect()),
            perm: self.perm,
            path,
            file_inserter: file_inserter as _,
        })
    }
}

pub enum RecursiveBuild {
    Down(String, Permission),
    Entry(String, Arsc<dyn Entry>),
    Up,
}

pub trait RecursiveBuilder: Iterator<Item = RecursiveBuild> + Sized {
    fn build(mut self, root_perm: Permission) -> Result<Arsc<MemDir>, Error> {
        let mut root = MemDir {
            entries: BTreeMap::new(),
            perm: root_perm,
        };
        build_recursive(&mut self, &mut root)?;
        Ok(Arsc::new(root))
    }

    fn build_mut<F: FileInserter + 'static>(
        mut self,
        root_perm: Permission,
        file_inserter: Arsc<F>,
    ) -> Result<Arsc<MemDirMut>, Error> {
        let mut root = MemDirMut {
            entries: Mutex::new(BTreeMap::new()),
            perm: root_perm,
            path: "".into(),
            file_inserter: file_inserter.clone(),
        };
        build_recursive_mut(&mut self, &mut root, file_inserter)?;
        Ok(Arsc::new(root))
    }
}

impl<T: Iterator<Item = RecursiveBuild> + Sized> RecursiveBuilder for T {}

fn build_recursive(
    iter: &mut impl Iterator<Item = RecursiveBuild>,
    dir: &mut MemDir,
) -> Result<(), Error> {
    while let Some(build) = iter.next() {
        match build {
            RecursiveBuild::Down(name, perm) => match dir.entries.entry(name) {
                MapEntry::Vacant(ent) => {
                    let mut sub = MemDir {
                        entries: BTreeMap::new(),
                        perm,
                    };
                    build_recursive(iter, &mut sub)?;
                    ent.insert(Arsc::new(sub));
                }
                MapEntry::Occupied(_) => return Err(Error::Exists),
            },
            RecursiveBuild::Entry(name, entry) => match dir.entries.entry(name) {
                MapEntry::Vacant(ent) => {
                    ent.insert(entry);
                }
                MapEntry::Occupied(_) => return Err(Error::Exists),
            },
            RecursiveBuild::Up => break,
        }
    }
    Ok(())
}

fn build_recursive_mut<F: FileInserter + 'static>(
    iter: &mut impl Iterator<Item = RecursiveBuild>,
    dir: &mut MemDirMut,
    file_inserter: Arsc<F>,
) -> Result<(), Error> {
    while let Some(build) = iter.next() {
        match build {
            RecursiveBuild::Down(name, perm) => match dir.entries.get_mut().entry(name.clone()) {
                MapEntry::Vacant(ent) => {
                    let mut sub = MemDirMut {
                        entries: Mutex::new(BTreeMap::new()),
                        perm,
                        path: dir.path.join(name),
                        file_inserter: file_inserter.clone(),
                    };
                    build_recursive_mut(iter, &mut sub, file_inserter.clone())?;
                    ent.insert(Arsc::new(sub));
                }
                MapEntry::Occupied(_) => return Err(Error::Exists),
            },
            RecursiveBuild::Entry(name, entry) => match dir.entries.get_mut().entry(name) {
                MapEntry::Vacant(ent) => {
                    ent.insert(entry);
                }
                MapEntry::Occupied(_) => return Err(Error::Exists),
            },
            RecursiveBuild::Up => break,
        }
    }
    Ok(())
}
