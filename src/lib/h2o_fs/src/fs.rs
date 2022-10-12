use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
};
use core::iter::Peekable;

use solvent_rpc::io::{
    entry::{entry_sync::EntryClient, EntryServer},
    Error, FileType, Metadata, OpenOptions,
};
use solvent_std::{
    path::{Component, Components, Path, PathBuf},
    sync::{Arsc, Mutex, MutexGuard},
};

use crate::entry::Entry;

enum Node {
    Dir(Mutex<BTreeMap<String, Arsc<Node>>>),
    Remote(EntryClient),
}

impl Clone for Node {
    fn clone(&self) -> Self {
        match self {
            Self::Dir(entries) => Self::Dir(Mutex::new(entries.lock().clone())),
            Self::Remote(remote) => Self::Remote(remote.clone()),
        }
    }
}

pub struct LocalFs {
    root: Arsc<Node>,
    cwd: Mutex<PathBuf>,
}

impl Clone for LocalFs {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            cwd: Mutex::new("".into()),
        }
    }
}

impl Entry for Node {
    #[inline]
    fn open(
        self: Arsc<Self>,
        path: &Path,
        options: OpenOptions,
        conn: EntryServer,
    ) -> Result<(), Error> {
        self.open(path, options, conn)
    }

    #[inline]
    fn metadata(&self) -> Result<Metadata, Error> {
        self.metadata()
    }
}

impl Node {
    #[inline]
    fn empty() -> Arsc<Self> {
        Arsc::new(Node::Dir(Mutex::new(BTreeMap::new())))
    }

    #[inline]
    fn leaf(remote: EntryClient) -> Arsc<Self> {
        Arsc::new(Node::Remote(remote))
    }

    fn metadata(&self) -> Result<Metadata, Error> {
        Ok(match self {
            Node::Dir(entries) => Metadata {
                file_type: FileType::Directory,
                len: entries.lock().len(),
            },
            Node::Remote(remote) => remote
                .metadata()
                .map_err(|err| Error::RpcError(err.to_string()))??,
        })
    }

    fn open(
        self: Arsc<Self>,
        path: &Path,
        options: OpenOptions,
        conn: EntryServer,
    ) -> Result<(), Error> {
        let (node, comps) = self.open_node(path, &mut None)?;
        match *node {
            Node::Dir(_) => Err(Error::InvalidType(FileType::Directory)),
            Node::Remote(ref remote) => {
                let path = PathBuf::from_iter(comps);
                remote.open(path, options, conn)?
            }
        }
    }

    fn remove(self: Arsc<Self>, path: &Path, all: bool) -> Result<(), Error> {
        let parent = path
            .parent()
            .ok_or_else(|| Error::PermissionDenied(Default::default()))?;
        let (parent, _) = self.open_node(parent, &mut None)?;
        let rest = path.file_name().unwrap().to_str().unwrap();
        let mut entries = if let Node::Dir(ref dir) = *parent {
            dir.lock()
        } else {
            return Err(Error::InvalidType(FileType::File));
        };
        if all {
            let _ = entries.remove(rest);
        } else if let Some(node) = entries.get(rest) {
            if let Node::Dir(..) = **node {
                return Err(Error::InvalidType(FileType::Directory));
            }
            let _ = entries.remove(rest);
        }
        Ok(())
    }

    #[inline]
    fn create(self: Arsc<Self>, path: &Path, remote: EntryClient) -> Result<(), Error> {
        let _ = self.open_node(path, &mut Some(remote))?;
        Ok(())
    }

    fn open_node<'a>(
        self: Arsc<Self>,
        path: &'a Path,
        create: &mut Option<EntryClient>,
    ) -> Result<(Arsc<Node>, Peekable<Components<'a>>), Error> {
        let mut node = self;
        let mut comps = path.components().peekable();
        while let Some(comp) = comps.next() {
            match comp {
                Component::Normal(comp) => {
                    let comp = comp.to_str().unwrap();

                    let child = if let Node::Dir(ref dir) = *node {
                        let entries = dir.lock();
                        match entries.get(comp) {
                            Some(child) => Arsc::clone(child),
                            None if create.is_some() => {
                                Self::create_node(comps.peek().is_none(), create, entries, comp)
                            }
                            None => {
                                drop(entries);
                                return Ok((node, comps));
                            }
                        }
                    } else {
                        return Err(Error::InvalidType(FileType::File));
                    };
                    node = child;
                }
                _ => unreachable!(),
            }
        }
        if create.is_some() {
            return Err(Error::Exists);
        }
        Ok((node, comps))
    }

    fn create_node(
        is_file: bool,
        create: &mut Option<EntryClient>,
        mut entries: MutexGuard<BTreeMap<String, Arsc<Node>>>,
        comp: &str,
    ) -> Arsc<Node> {
        if is_file {
            let new = Self::leaf(create.take().unwrap());
            entries.insert(comp.to_string(), Arsc::clone(&new));
            new
        } else {
            let new = Self::empty();
            entries.insert(comp.to_string(), Arsc::clone(&new));
            new
        }
    }
}

impl LocalFs {
    pub fn new() -> Self {
        LocalFs {
            root: Node::empty(),
            cwd: Mutex::new("".into()),
        }
    }

    fn node_and_path(&self, path: &Path) -> Result<(PathBuf, Arsc<Node>), Error> {
        let path =
            canonicalize(path, &self.cwd).ok_or_else(|| Error::InvalidPath(path.to_path_buf()))?;
        Ok((path, self.root.clone()))
    }

    pub fn open(&self, path: &Path, options: OpenOptions, conn: EntryServer) -> Result<(), Error> {
        let (path, node) = self.node_and_path(path)?;
        node.open(&path, options, conn)
    }

    pub fn cd(&self, path: &Path) -> Result<(), Error> {
        let path =
            canonicalize(path, &self.cwd).ok_or_else(|| Error::InvalidPath(path.to_path_buf()))?;
        *self.cwd.lock() = path;
        Ok(())
    }

    pub fn mount(&self, path: &Path, remote: EntryClient) -> Result<(), Error> {
        let (path, node) = self.node_and_path(path)?;
        node.create(&path, remote)
    }

    pub fn unmount(&self, path: &Path, all: bool) -> Result<(), Error> {
        let (path, node) = self.node_and_path(path)?;
        node.remove(&path, all)
    }
}

impl Default for LocalFs {
    fn default() -> Self {
        Self::new()
    }
}

/// # Returns
///
/// The canonicalized path and whether the original has a root prefix.
fn canonicalize(path: &Path, cwd: &Mutex<PathBuf>) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.lock().join(path)
    };
    for comp in path.components() {
        match comp {
            Component::Prefix(_) => return None,
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::Normal(comp) => out.push(comp.to_str()?),
        }
    }
    Some(out)
}
