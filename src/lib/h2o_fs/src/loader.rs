use alloc::vec::Vec;

use futures::StreamExt;
use solvent::prelude::Phys;
use solvent_core::{ffi::OsStr, path::Path};
use solvent_rpc::{
    io::{
        dir::DirectoryClient,
        file::{File, PhysOptions},
        Error, OpenOptions,
    },
    loader::{LoaderRequest, LoaderServer},
    Protocol, Server,
};

pub async fn get_object_from_dir<D: AsRef<DirectoryClient>, P: AsRef<Path>>(
    dir: D,
    path: P,
) -> Result<Phys, Error> {
    let (file, server) = File::channel();
    let dir = dir.as_ref();
    dir.open(
        path.as_ref().into(),
        OpenOptions::READ,
        server.try_into().unwrap(),
    )
    .await??;
    file.phys(PhysOptions::Copy).await?
}

pub async fn get_object<D: AsRef<DirectoryClient>, P: AsRef<Path>>(
    dir: impl Iterator<Item = D>,
    path: P,
) -> Option<Phys> {
    let path = path.as_ref();
    for dir in dir {
        match get_object_from_dir(dir, path).await {
            Ok(phys) => return Some(phys),
            Err(err) => log::warn!("Failed to get object from {path:?}: {err}"),
        }
    }
    None
}

pub async fn serve<D: AsRef<DirectoryClient>>(
    server: LoaderServer,
    dir: impl Iterator<Item = D> + Clone,
) {
    let (mut request, _) = server.serve();
    while let Some(request) = request.next().await {
        let request = match request {
            Ok(request) => request,
            Err(err) => {
                log::warn!("RPC receive error: {err}");
                continue;
            }
        };
        match request {
            LoaderRequest::GetObject { path, responder } => {
                let dir = dir.clone();
                let fut = async move {
                    let mut ret = Vec::new();
                    for (index, path) in path.into_iter().enumerate() {
                        match get_object(dir.clone(), OsStr::from_bytes(path.as_bytes())).await {
                            Some(obj) => ret.push(obj),
                            None => return Err(index),
                        }
                    }
                    Ok(ret)
                };
                let res = responder.send(fut.await);
                if let Err(err) = res {
                    log::warn!("RPC send error: {err}");
                }
            }
            LoaderRequest::Unknown(_) => {
                log::warn!("RPC received unknown request")
            }
        }
    }
}
