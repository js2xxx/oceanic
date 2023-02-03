use alloc::{
    collections::{btree_map::Entry as MapEntry, BTreeMap},
    ffi::{CString, FromVecWithNulError},
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::{mem, num::NonZeroUsize, ptr::NonNull};

use solvent::{
    prelude::{drop_raw, Channel, Feature, Flags, Handle, Object, Phys, Space, Virt, PAGE_SIZE},
    task::{Task, DEFAULT_STACK_SIZE},
};
use solvent_async::disp::DispSender;
use solvent_core::{path::PathBuf, sync::Lazy};
#[cfg(feature = "runtime")]
use solvent_rpc::{io::dir::DirectoryClient, loader::Loader, Protocol};
use solvent_rpc::{
    io::entry::EntrySyncClient,
    loader::{LoaderClient, LoaderSyncClient},
    sync::Client as SyncClient,
    Client,
};
use svrt::{HandleInfo, HandleType, StartupArgs};

use super::{InitProcess, Process};

const INTERP: &str = "lib/ld-oceanic.so";

#[derive(Debug)]
pub enum Error {
    LoadPhys(elfload::Error),
    FieldMissing(&'static str),
    InvalidCStr(FromVecWithNulError),
    DepNotFound(CString),
    Rpc(solvent_rpc::Error),
    VdsoMap(solvent::error::Error),
    StackAlloc(solvent::error::Error),
    SendStartupArgs(solvent::error::Error),
    TaskExec(solvent::error::Error),
}

impl From<elfload::Error> for Error {
    #[inline]
    fn from(value: elfload::Error) -> Self {
        Error::LoadPhys(value)
    }
}

#[derive(Default)]
pub struct Builder {
    local_fs: BTreeMap<PathBuf, EntrySyncClient>,
    handles: BTreeMap<HandleInfo, Handle>,
    executable: Option<(Phys, String)>,
    loader: Option<LoaderSyncClient>,
    vdso: Option<Phys>,
    args: Vec<String>,
    environ: BTreeMap<String, String>,
}

impl Builder {
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    #[inline]
    pub fn local_fs<I>(&mut self, iter: I) -> &mut Self
    where
        I: IntoIterator<Item = (PathBuf, EntrySyncClient)>,
    {
        self.local_fs.extend(iter);
        self
    }

    /// # Safety
    ///
    /// The caller must ensure that the handles are with their own ownerships.
    #[inline]
    pub unsafe fn handles<I>(&mut self, iter: I) -> &mut Self
    where
        I: Iterator<Item = (HandleInfo, Handle)>,
    {
        self.handles.extend(iter);
        self
    }

    pub fn executable(
        &mut self,
        executable: Phys,
        name: impl Into<String>,
    ) -> Result<&mut Self, Phys> {
        fn inner(this: &mut Builder, executable: Phys, name: String) -> Result<&mut Builder, Phys> {
            if this.executable.is_some() {
                return Err(executable);
            }
            let executable = executable
                .reduce_features(Feature::SEND | Feature::READ | Feature::EXECUTE)
                .expect("Failed to adjust features for executable");
            this.executable = Some((executable, name.clone()));
            this.args.insert(0, name);
            Ok(this)
        }
        inner(self, executable, name.into())
    }

    pub fn loader_sync(&mut self, loader: LoaderSyncClient) -> Result<&mut Self, LoaderSyncClient> {
        if self.loader.is_some() {
            return Err(loader);
        }
        self.loader = Some(loader);
        Ok(self)
    }

    pub fn loader(&mut self, loader: LoaderClient) -> Result<&mut Self, LoaderClient> {
        if self.loader.is_some() {
            return Err(loader);
        }
        self.loader = Some(loader.into_sync().unwrap());
        Ok(self)
    }

    #[cfg(feature = "runtime")]
    pub fn load_dirs(
        &mut self,
        dirs: Vec<DirectoryClient>,
    ) -> Result<&mut Self, Vec<DirectoryClient>> {
        if self.loader.is_some() {
            return Err(dirs);
        }
        let (client, server) = Loader::channel();
        let task = crate::loader::serve(solvent_async::dispatch(), server, dirs.into_iter());
        solvent_async::spawn(task).detach();

        Ok(self.loader(client).unwrap())
    }

    #[inline]
    pub fn vdso(&mut self, vdso: Phys) -> &mut Self {
        self.vdso = Some(vdso);
        self
    }

    #[inline]
    pub fn args<S, I>(&mut self, args: I) -> &mut Self
    where
        S: Into<String>,
        I: IntoIterator<Item = S>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    #[inline]
    pub fn arg<S>(&mut self, arg: S) -> &mut Self
    where
        S: Into<String>,
    {
        self.args.push(arg.into());
        self
    }

    pub fn environ<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.environ.insert(key.into(), value.into());
        self
    }

    pub fn environs<K, V, I>(&mut self, iter: I) -> &mut Self
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        self.environ.extend(
            iter.into_iter()
                .map(|(key, value)| (key.into(), value.into())),
        );
        self
    }

    #[inline]
    pub fn append_environ<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: Into<String>,
        V: AsRef<str>,
    {
        append_environ(&mut self.environ, key.into(), value.as_ref());
        self
    }

    pub fn append_environs<K, V, I>(&mut self, iter: I) -> &mut Self
    where
        K: Into<String>,
        V: AsRef<str>,
        I: IntoIterator<Item = (K, V)>,
    {
        for (key, value) in iter {
            self.append_environ(key, value);
        }
        self
    }

    fn build_args_sync(&mut self) -> Result<BuildArgs, Error> {
        let Builder {
            local_fs,
            handles,
            executable,
            loader,
            vdso,
            args,
            environ,
        } = mem::take(self);
        let (executable, name) = executable.ok_or_else(|| Error::FieldMissing("executable"))?;
        let loader = loader.ok_or_else(|| Error::FieldMissing("loader"))?;
        let vdso = vdso.unwrap_or_else(self::vdso);

        let interp_path = match elfload::get_interp(&executable)? {
            Some(bytes) => CString::from_vec_with_nul(bytes),
            None => Ok(CString::new(INTERP).unwrap()),
        }
        .map_err(Error::InvalidCStr)?;

        let interp = loader
            .get_object(vec![interp_path.clone()])
            .map_err(Error::Rpc)?
            .map_err(|_| Error::DepNotFound(interp_path))?
            .pop()
            .unwrap();

        build_end(
            interp, executable, vdso, loader, handles, local_fs, args, environ, name,
        )
    }

    async fn build_args(&mut self, disp: DispSender) -> Result<BuildArgs, Error> {
        let Builder {
            local_fs,
            handles,
            executable,
            loader,
            vdso,
            args,
            environ,
        } = mem::take(self);
        let (executable, name) = executable.ok_or_else(|| Error::FieldMissing("executable"))?;
        let loader = loader
            .ok_or_else(|| Error::FieldMissing("loader"))?
            .into_async_with_disp(disp)
            .unwrap();
        let vdso = vdso.unwrap_or_else(self::vdso);

        let interp_path = match elfload::get_interp(&executable)? {
            Some(bytes) => CString::from_vec_with_nul(bytes),
            None => Ok(CString::new(INTERP).unwrap()),
        }
        .map_err(Error::InvalidCStr)?;

        let interp = loader
            .get_object(vec![interp_path.clone()])
            .await
            .map_err(Error::Rpc)?
            .map_err(|_| Error::DepNotFound(interp_path))?
            .pop()
            .unwrap();

        let loader = solvent_rpc::Client::into_sync(loader).unwrap();
        build_end(
            interp, executable, vdso, loader, handles, local_fs, args, environ, name,
        )
    }

    pub fn build_sync(&mut self) -> Result<Process, Error> {
        let build_args = self.build_args_sync()?;

        let proc = Process::new(
            Task::exec(
                Some(&build_args.name),
                Some(build_args.space),
                build_args.entry,
                build_args.stack,
                Some(build_args.init_chan),
                build_args.vdso_base.as_ptr() as _,
            )
            .map_err(Error::TaskExec)?,
        );

        Ok(proc)
    }

    pub fn build_non_start_sync(&mut self) -> Result<InitProcess, Error> {
        let build_args = self.build_args_sync()?;

        let (task, suspend_token) = Task::new(
            Some(&build_args.name),
            Some(build_args.space),
            Some(build_args.init_chan),
        );
        let proc = InitProcess {
            task,
            entry: build_args.entry,
            stack: build_args.stack,
            vdso_base: build_args.vdso_base,
            suspend_token,
        };

        Ok(proc)
    }

    pub async fn build_with_disp(&mut self, disp: DispSender) -> Result<Process, Error> {
        let build_args = self.build_args(disp).await?;

        let proc = Process::new(
            Task::exec(
                Some(&build_args.name),
                Some(build_args.space),
                build_args.entry,
                build_args.stack,
                Some(build_args.init_chan),
                build_args.vdso_base.as_ptr() as _,
            )
            .map_err(Error::TaskExec)?,
        );

        Ok(proc)
    }

    pub async fn build_non_start_with_disp(
        &mut self,
        disp: DispSender,
    ) -> Result<InitProcess, Error> {
        let build_args = self.build_args(disp).await?;

        let (task, suspend_token) = Task::new(
            Some(&build_args.name),
            Some(build_args.space),
            Some(build_args.init_chan),
        );
        let proc = InitProcess {
            task,
            entry: build_args.entry,
            stack: build_args.stack,
            vdso_base: build_args.vdso_base,
            suspend_token,
        };

        Ok(proc)
    }

    #[cfg(feature = "runtime")]
    pub async fn build(&mut self) -> Result<Process, Error> {
        self.build_with_disp(solvent_async::dispatch()).await
    }

    #[cfg(feature = "runtime")]
    pub async fn build_non_start(&mut self) -> Result<InitProcess, Error> {
        self.build_non_start_with_disp(solvent_async::dispatch())
            .await
    }
}

struct BuildArgs {
    name: String,
    space: Space,
    entry: NonNull<u8>,
    stack: NonNull<u8>,
    init_chan: Channel,
    vdso_base: NonNull<u8>,
}

#[allow(clippy::too_many_arguments)]
fn build_end(
    interp: Phys,
    executable: Phys,
    vdso: Phys,
    loader: LoaderSyncClient,
    handles: BTreeMap<HandleInfo, Handle>,
    local_fs: BTreeMap<PathBuf, EntrySyncClient>,
    args: Vec<String>,
    environ: BTreeMap<String, String>,
    name: String,
) -> Result<BuildArgs, Error> {
    let (space, root_virt) = Space::new();

    let loaded = elfload::load(&interp, true, &root_virt)?;
    elfload::load(&executable, true, &root_virt)?;

    let vdso_base = root_virt.map_vdso(vdso.clone()).map_err(Error::VdsoMap)?;

    let (stack_size, stack_flags) = pass_stack(loaded.stack);

    let stack = allocate_stack(&root_virt, stack_size, stack_flags).map_err(Error::StackAlloc)?;

    let entry = unsafe { NonNull::new_unchecked(loaded.entry as *mut u8) };

    let (me, child) = Channel::new();

    let child = child
        .reduce_features(Feature::SEND | Feature::READ)
        .expect("Failed to reduce features for read");
    let me = me
        .reduce_features(Feature::SEND | Feature::WRITE)
        .expect("Failed to reduce features for write");

    let dl_args = StartupArgs {
        handles: [
            (
                HandleType::RootVirt.into(),
                Virt::into_raw(root_virt.clone()),
            ),
            (HandleType::VdsoPhys.into(), Phys::into_raw(vdso.clone())),
            (HandleType::ProgramPhys.into(), Phys::into_raw(executable)),
            (
                HandleType::LoadRpc.into(),
                Channel::into_raw(loader.try_into().unwrap()),
            ),
        ]
        .into_iter()
        .collect(),
        args: vec![0],
        env: vec![0],
    };

    let mut packet = Default::default();
    dl_args
        .send(&me, &mut packet)
        .map_err(Error::SendStartupArgs)?;

    startup_args(handles, local_fs, args, environ, root_virt, vdso)
        .send(&me, &mut packet)
        .map_err(Error::SendStartupArgs)?;

    Ok(BuildArgs {
        name,
        space,
        entry,
        stack,
        init_chan: child,
        vdso_base: vdso_base.as_non_null_ptr(),
    })
}

fn vdso() -> Phys {
    static VDSO: Lazy<Phys> = Lazy::new(|| unsafe {
        Phys::from_raw(svrt::take_startup_handle(HandleType::VdsoPhys.into()))
    });
    VDSO.clone()
}

fn allocate_stack(
    root_virt: &Virt,
    size: usize,
    flags: Flags,
) -> Result<NonNull<u8>, solvent::error::Error> {
    unsafe {
        let virt = root_virt.allocate(None, Virt::page_aligned(size + 2 * PAGE_SIZE))?;

        let phys = Phys::allocate(size, Default::default())?;

        let range = virt.map_phys(Some(PAGE_SIZE), phys, flags)?;

        Ok(NonNull::new_unchecked(range.as_mut_ptr().add(range.len())))
    }
}

fn append_environ(environ: &mut BTreeMap<String, String>, key: String, value: &str) {
    match environ.entry(key) {
        MapEntry::Vacant(ent) => {
            ent.insert(value.to_string());
        }
        MapEntry::Occupied(mut ent) => {
            ent.get_mut().push(',');
            *ent.get_mut() += value
        }
    }
}

#[inline]
fn pass_stack(stack: Option<(usize, Flags)>) -> (usize, Flags) {
    let flags = Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS;
    stack.map_or((DEFAULT_STACK_SIZE, flags), |stack| {
        (
            NonZeroUsize::new(stack.0).map_or(DEFAULT_STACK_SIZE, |size| size.get()),
            stack.1,
        )
    })
}

fn startup_args(
    mut handles: BTreeMap<HandleInfo, Handle>,
    local_fs: BTreeMap<PathBuf, EntrySyncClient>,
    args: Vec<String>,
    mut environ: BTreeMap<String, String>,
    root_virt: Virt,
    vdso: Phys,
) -> StartupArgs {
    local_fs
        .into_iter()
        .enumerate()
        .for_each(|(index, (path, entry))| {
            let hinfo = HandleInfo::new()
                .with_handle_type(HandleType::LocalFs)
                .with_additional(index as u16);
            let handle = Channel::into_raw(entry.try_into().unwrap());

            append_environ(&mut environ, "LFS".into(), &path.to_string_lossy());
            if let Some(old) = handles.insert(hinfo, handle) {
                let _ = unsafe { drop_raw(old) };
            }
        });
    handles.insert(HandleType::RootVirt.into(), Virt::into_raw(root_virt));
    handles.insert(HandleType::VdsoPhys.into(), Phys::into_raw(vdso));
    let args = args
        .into_iter()
        .flat_map(|arg| arg.into_bytes().into_iter().chain([0]))
        .collect::<Vec<_>>();
    let environ = environ
        .into_iter()
        .flat_map(|(key, value)| {
            key.into_bytes()
                .into_iter()
                .chain([b'='])
                .chain(value.into_bytes())
                .chain([0])
        })
        .collect::<Vec<_>>();
    StartupArgs {
        handles,
        args,
        env: environ,
    }
}
