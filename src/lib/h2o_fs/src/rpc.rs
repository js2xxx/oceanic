use core::{future::Future, marker::PhantomData};

use solvent::prelude::Channel;
use solvent_async::ipc::Channel as AsyncChannel;
use solvent_core::{path::Path, sync::Arsc};
use solvent_rpc::{
    io::{Error, FileType, Metadata, OpenOptions, Permission},
    Server,
};

use crate::{dir::EventTokens, entry::Entry, spawn::Spawner};

pub struct RpcNode<S, G, F>
where
    S: Server + Send + Sync + 'static,
    G: Fn(S) -> F + Sync + Send + 'static,
    F: Future<Output = ()> + Sync + Send + 'static,
{
    gen: G,
    _marker: PhantomData<S>,
}

impl<S, G, F> RpcNode<S, G, F>
where
    S: Server + Send + Sync + 'static,
    G: Fn(S) -> F + Sync + Send + 'static,
    F: Future<Output = ()> + Sync + Send + 'static,
{
    pub fn new(func: G) -> Arsc<Self> {
        Arsc::new(RpcNode {
            gen: func,
            _marker: PhantomData,
        })
    }
}

impl<S, G, F> Entry for RpcNode<S, G, F>
where
    S: Server + Send + Sync + 'static,
    G: Fn(S) -> F + Sync + Send + 'static,
    F: Future<Output = ()> + Sync + Send + 'static,
{
    fn open(
        self: Arsc<Self>,
        spawner: Spawner,
        _: EventTokens,
        path: &Path,
        options: OpenOptions,
        conn: Channel,
    ) -> Result<bool, Error> {
        if options - OpenOptions::EXPECT_RPC != OpenOptions::READ | OpenOptions::WRITE {
            return Err(Error::PermissionDenied(options.require()));
        }
        if path != Path::new("") {
            return Err(Error::InvalidPath(path.into()));
        }
        let server = S::from(AsyncChannel::with_disp(conn, spawner.dispatch()));
        let task = (self.gen)(server);
        spawner.spawn(task);
        Ok(false)
    }

    fn metadata(&self) -> Result<Metadata, Error> {
        Ok(Metadata {
            file_type: FileType::RpcNode,
            perm: Permission::READ | Permission::WRITE,
            len: 0,
        })
    }
}
