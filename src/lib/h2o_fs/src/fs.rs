use alloc::{
    collections::{btree_map, BTreeMap},
    string::{String, ToString},
};
use core::{iter::Peekable, sync::atomic};

use solvent::{
    ipc::Channel,
    prelude::{Handle, Object},
};
use solvent_core::{
    path::{Component, Components, Path, PathBuf},
    sync::{Arsc, Mutex, MutexGuard},
};
use solvent_rpc::io::{
    dir::{DirEntry, DirectorySyncClient},
    entry::EntrySyncClient,
    file::FileSyncClient,
    Error, FileType, Metadata, OpenOptions, Permission,
};

use crate::dir::sync::RemoteIter;

enum Node {
    Dir(Mutex<BTreeMap<String, Arsc<Node>>>),
    Remote(EntrySyncClient),
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

impl Node {
    #[inline]
    fn empty() -> Arsc<Self> {
        Arsc::new(Node::Dir(Mutex::new(BTreeMap::new())))
    }

    #[inline]
    fn leaf(remote: EntrySyncClient) -> Arsc<Self> {
        Arsc::new(Node::Remote(remote))
    }

    fn metadata(&self) -> Result<Metadata, Error> {
        Ok(match self {
            Node::Dir(entries) => Metadata {
                file_type: FileType::Directory,
                perm: Permission::all(),
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
        conn: Channel,
    ) -> Result<(), Error> {
        let (node, comps) = self.open_node(path, &mut None)?;
        match *node {
            Node::Dir(_) => Err(Error::LocalFs(path.to_path_buf())),
            Node::Remote(ref remote) => {
                let path = PathBuf::from_iter(comps);
                remote.open(path, options, conn)?
            }
        }
    }

    fn remove(self: Arsc<Self>, path: &Path, all: bool) -> Result<(), Error> {
        let parent_path = path
            .parent()
            .ok_or_else(|| Error::PermissionDenied(Default::default()))?;
        let (parent, _) = self.open_node(parent_path, &mut None)?;
        let rest = path.file_name().unwrap().to_str().unwrap();
        let mut entries = match *parent {
            Node::Dir(ref dir) => dir.lock(),
            _ => return Err(Error::LocalFs(parent_path.into())),
        };
        let old = if all {
            entries.remove(rest)
        } else if let Some(node) = entries.get(rest) {
            if let Node::Dir(..) = **node {
                return Err(Error::InvalidType(FileType::Directory));
            }
            entries.remove(rest)
        } else {
            None
        };
        old.map(drop).ok_or(Error::NotFound)
    }

    #[inline]
    fn create(self: Arsc<Self>, path: &Path, remote: EntrySyncClient) -> Result<(), Error> {
        let _ = self.open_node(path, &mut Some(remote))?;
        Ok(())
    }

    fn open_node<'a>(
        self: Arsc<Self>,
        path: &'a Path,
        create: &mut Option<EntrySyncClient>,
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
                        return Err(Error::LocalFs(path.into()));
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
        create: &mut Option<EntrySyncClient>,
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

    fn export(
        self: Arsc<Self>,
        path: PathBuf,
        output: &mut impl Extend<(PathBuf, EntrySyncClient)>,
    ) -> Result<(), Error> {
        match *self {
            Node::Dir(ref dir) => {
                let entries = dir.lock();
                output.extend_reserve(entries.len().saturating_sub(1));
                for (name, node) in entries.iter() {
                    node.clone().export(path.join(name), output)?;
                }
            }
            Node::Remote(ref remote) => {
                let (t, conn) = Channel::new();
                remote.clone_connection(conn)?;
                let remote = EntrySyncClient::from(t);
                output.extend(Some((path, remote)));
            }
        }
        Ok(())
    }

    fn copy_local(
        self: Arsc<Self>,
        src: PathBuf,
        dst: PathBuf,
        remove_src: bool,
    ) -> Result<(), Error> {
        let (src_parent, src_file) = match (src.parent(), src.file_name()) {
            (Some(parent), Some(file)) => match file.to_str() {
                Some(file) => (parent, file),
                None => return Err(Error::InvalidPath(src)),
            },
            _ => return Err(Error::InvalidPath(src)),
        };

        let (dst_parent, dst_file) = match (dst.parent(), dst.file_name()) {
            (Some(parent), Some(file)) => (parent, file),
            _ => return Err(Error::InvalidPath(dst)),
        };

        let (t, conn) = Channel::new();
        self.clone()
            .open(src_parent, OpenOptions::READ | OpenOptions::WRITE, conn)?;
        let src_parent = DirectorySyncClient::from(t);

        let (t, conn) = Channel::new();
        self.open(dst_parent, OpenOptions::READ | OpenOptions::WRITE, conn)?;
        let dst_parent = DirectorySyncClient::from(t);

        {
            let (t, conn) = Channel::new();
            src_parent.open(
                src_file.into(),
                OpenOptions::READ | OpenOptions::WRITE,
                conn,
            )??;
            let src_file = FileSyncClient::from(t);

            let (t, conn) = Channel::new();
            dst_parent.open(
                dst_file.into(),
                OpenOptions::CREATE_NEW | OpenOptions::READ | OpenOptions::WRITE,
                conn,
            )??;
            let dst_file = FileSyncClient::from(t);

            let src_metadata = src_file.metadata()??;
            if src_metadata.file_type != FileType::File {
                return Err(Error::InvalidType(src_metadata.file_type));
            }
            let dst_metadata = dst_file.metadata()??;
            if dst_metadata.file_type != FileType::File {
                return Err(Error::InvalidType(dst_metadata.file_type));
            }

            let len = src_metadata.len;
            let content = src_file.read(len)??;
            let written_len = dst_file.write(content)??;
            assert_eq!(written_len, len);

            src_file.close_connection()?;
            dst_file.close_connection()?
        }

        if remove_src {
            src_parent.unlink(src_file.into(), false)??;
        }

        Ok(())
    }
}

impl LocalFs {
    pub fn new() -> Self {
        LocalFs {
            root: Node::empty(),
            cwd: Mutex::new("".into()),
        }
    }

    fn canonicalize_with(path: &Path, cwd: &Path) -> Result<PathBuf, Error> {
        fn inner(path: &Path, cwd: &Path) -> Option<PathBuf> {
            let mut out = PathBuf::new();
            let path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
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
        inner(path, cwd).ok_or_else(|| Error::InvalidPath(path.to_path_buf()))
    }

    #[inline]
    pub fn canonicalize<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, Error> {
        Self::canonicalize_with(path.as_ref(), &self.cwd.lock())
    }

    #[inline]
    pub fn open<P: AsRef<Path>>(
        &self,
        path: P,
        options: OpenOptions,
        conn: Channel,
    ) -> Result<(), Error> {
        let path = self.canonicalize(path)?;
        self.root.clone().open(&path, options, conn)
    }

    pub fn chdir<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let path = self.canonicalize(path)?;
        *self.cwd.lock() = path;
        Ok(())
    }

    #[inline]
    pub fn mount<P: AsRef<Path>>(&self, path: P, remote: EntrySyncClient) -> Result<(), Error> {
        let path = self.canonicalize(path)?;
        self.root.clone().create(&path, remote)
    }

    #[inline]
    pub fn unmount<P: AsRef<Path>>(&self, path: P, all: bool) -> Result<(), Error> {
        let path = self.canonicalize(path)?;
        self.root.clone().remove(&path, all)
    }

    pub fn metadata<P: AsRef<Path>>(&self, path: P) -> Result<Metadata, Error> {
        let path = self.canonicalize(path)?;
        let (node, mut comps) = self.root.clone().open_node(&path, &mut None)?;
        match *node {
            Node::Dir(..) => Ok(node.metadata()?),
            Node::Remote(ref remote) if comps.peek().is_some() => {
                let path = PathBuf::from_iter(comps);
                let (t, conn) = Channel::new();
                let client = EntrySyncClient::from(t);
                remote.open(path, OpenOptions::READ, conn)??;
                client.metadata()?
            }
            Node::Remote(ref remote) => remote.metadata()?,
        }
    }

    pub fn read_dir<P: AsRef<Path>>(&self, path: P) -> Result<DirIter, Error> {
        let path = self.canonicalize(path)?;
        let (node, mut comps) = self.root.clone().open_node(&path, &mut None)?;
        match *node {
            Node::Dir(..) => {
                let builder = LocalIterBuilder {
                    node,
                    guard_builder: |node: &Arsc<Node>| match **node {
                        Node::Dir(ref dir) => dir.lock(),
                        _ => unreachable!(),
                    },
                    iter_builder: |guard| guard.iter(),
                };
                Ok(DirIter::Local(builder.build()))
            }
            Node::Remote(ref remote) if comps.peek().is_some() => {
                let path = PathBuf::from_iter(comps);
                let (t, conn) = Channel::new();
                remote.open(path, OpenOptions::READ, conn)??;
                Ok(DirIter::Remote(DirectorySyncClient::from(t).into()))
            }
            Node::Remote(ref remote) => {
                let metadata = remote.metadata()??;
                if metadata.file_type != FileType::Directory {
                    return Err(Error::InvalidType(metadata.file_type));
                }
                let (t, conn) = Channel::new();
                remote.clone_connection(conn)?;
                Ok(DirIter::Remote(DirectorySyncClient::from(t).into()))
            }
        }
    }

    pub fn export(
        &self,
        output: &mut impl Extend<(PathBuf, EntrySyncClient)>,
    ) -> Result<(), Error> {
        self.root.clone().export(PathBuf::new(), output)
    }

    pub fn unlink<P: AsRef<Path>>(&self, path: P, expect_dir: bool) -> Result<(), Error> {
        let path = self.canonicalize(path)?;
        let parent = path
            .parent()
            .ok_or_else(|| Error::PermissionDenied(Default::default()))?;

        let (t, conn) = Channel::new();
        let dir = DirectorySyncClient::from(t);
        self.open(parent, OpenOptions::READ | OpenOptions::WRITE, conn)?;

        let file_name = path
            .file_name()
            .and_then(|s| s.to_str().map(|s| s.to_string()));
        let file_name = file_name.ok_or(Error::InvalidPath(path))?;
        dir.unlink(file_name, expect_dir)?
    }

    fn two_path_op<Op, OpLocal>(
        &self,
        src: &Path,
        dst: &Path,
        op: Op,
        op_local: OpLocal,
    ) -> Result<(), Error>
    where
        Op: FnOnce(DirectorySyncClient, String, Handle, String) -> Result<(), Error>,
        OpLocal: FnOnce(Arsc<Node>, PathBuf, PathBuf, PathBuf) -> Result<(), Error>,
    {
        let cwd = self.cwd.lock();
        let src = Self::canonicalize_with(src, &cwd)?;
        let dst = Self::canonicalize_with(dst, &cwd)?;
        drop(cwd);

        {
            let mut lcp = PathBuf::new();
            for (old, new) in src.iter().zip(&dst) {
                if old == new {
                    lcp.push(old);
                } else {
                    break;
                }
            }
            let src_inner = src.strip_prefix(&lcp).unwrap();
            let dst_inner = dst.strip_prefix(&lcp).unwrap();
            if src_inner == Path::new("") {
                return Err(Error::IsAncestorOrEquals {
                    ancestor: src,
                    descendant: dst,
                });
            }
            if dst_inner == Path::new("") {
                return Err(Error::Exists);
            }

            let (node, _) = self.root.clone().open_node(&lcp, &mut None)?;
            if let Node::Dir(..) = *node {
                return op_local(node, lcp, src_inner.into(), dst_inner.into());
            }
        }

        let (src_parent, src_file) = match (src.parent(), src.file_name()) {
            (Some(parent), Some(file)) => match file.to_str() {
                Some(file) => (parent, file),
                None => return Err(Error::InvalidPath(src)),
            },
            _ => return Err(Error::InvalidPath(src)),
        };

        let (dst_parent, dst_file) = match (dst.parent(), dst.file_name()) {
            (Some(parent), Some(file)) => match file.to_str() {
                Some(file) => (parent, file),
                None => return Err(Error::InvalidPath(src)),
            },
            _ => return Err(Error::InvalidPath(dst)),
        };

        let (t, conn) = Channel::new();
        self.clone()
            .open(src_parent, OpenOptions::READ | OpenOptions::WRITE, conn)?;
        let src_parent = DirectorySyncClient::from(t);

        let dst_parent = {
            let (t, conn) = Channel::new();
            self.open(dst_parent, OpenOptions::READ | OpenOptions::WRITE, conn)?;
            let dst_parent = DirectorySyncClient::from(t);
            dst_parent.event_token()??
        };

        op(src_parent, src_file.into(), dst_parent, dst_file.into())
    }

    #[inline]
    pub fn rename<P1: AsRef<Path>, P2: AsRef<Path>>(&self, src: P1, dst: P2) -> Result<(), Error> {
        self.two_path_op(
            src.as_ref(),
            dst.as_ref(),
            |dir, src, dst_parent, dst| dir.rename(src, dst_parent, dst)?,
            |node, _, src, dst| node.copy_local(src, dst, true),
        )
    }

    #[inline]
    pub fn link<P1: AsRef<Path>, P2: AsRef<Path>>(&self, src: P1, dst: P2) -> Result<(), Error> {
        self.two_path_op(
            src.as_ref(),
            dst.as_ref(),
            |dir, src, dst_parent, dst| dir.link(src, dst_parent, dst)?,
            |_, lcp, _, _| Err(Error::LocalFs(lcp)),
        )
    }
}

#[ouroboros::self_referencing]
pub struct LocalIter {
    node: Arsc<Node>,
    #[borrows(node)]
    #[covariant]
    guard: MutexGuard<'this, BTreeMap<String, Arsc<Node>>>,
    #[borrows(guard)]
    #[covariant]
    iter: btree_map::Iter<'this, String, Arsc<Node>>,
}

impl Iterator for LocalIter {
    type Item = Result<DirEntry, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.with_iter_mut(|iter| {
            let (name, node) = iter.next()?;
            let metadata = node.metadata();
            Some(metadata.map(|metadata| DirEntry {
                name: name.to_string(),
                metadata,
            }))
        })
    }
}

pub enum DirIter {
    Local(LocalIter),
    Remote(RemoteIter),
}

impl Iterator for DirIter {
    type Item = Result<DirEntry, Error>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            DirIter::Local(local) => local.next(),
            DirIter::Remote(remote) => remote.next(),
        }
    }
}

impl Default for LocalFs {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

static mut LOCAL_FS: Option<LocalFs> = None;

/// # Safety
///
/// The function must be called only during the initialization of the whole
/// process.
pub unsafe fn init_rt(
    handles: &mut BTreeMap<svrt::HandleInfo, Handle>,
    paths: impl Iterator<Item = impl AsRef<Path>>,
    cwd: Option<impl AsRef<Path>>,
) {
    let iter = handles.drain_filter(|info, _| info.handle_type() == svrt::HandleType::LocalFs);

    let local_fs = LocalFs::new();
    for ((_, handle), path) in iter.zip(paths) {
        let remote = EntrySyncClient::from(unsafe { Channel::from_raw(handle) });
        let res = local_fs.mount(path, remote);
        if let Err(err) = res {
            log::warn!("Error when mounting the local FS: {err}");
        }
    }
    if let Some(cwd) = cwd {
        let path = cwd.as_ref();
        let res = local_fs.chdir(path);
        if let Err(err) = res {
            log::warn!("Error when cwding the local FS to {path:?}: {err}");
        }
    }

    let old = LOCAL_FS.replace(local_fs);
    atomic::fence(atomic::Ordering::Release);

    assert!(
        old.is_none(),
        "The local FS should only be initialized once"
    );
}

/// # Safety
///
/// The function must be called only during the finalization of the whole
/// process.
pub unsafe fn fini_rt() {
    LOCAL_FS = None;
}

#[inline]
pub fn local() -> &'static LocalFs {
    // SAFETY: The local FS should be initialized before `main`.
    unsafe { LOCAL_FS.as_ref().expect("The local FS is uninitialized") }
}
