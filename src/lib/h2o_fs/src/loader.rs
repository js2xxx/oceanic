use alloc::vec::Vec;
use core::borrow::Borrow;

use futures::StreamExt;
use solvent::prelude::Phys;
use solvent_async::disp::DispSender;
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

pub async fn get_object_from_dir<D: Borrow<DirectoryClient>, P: AsRef<Path>>(
    disp: DispSender,
    dir: D,
    path: P,
) -> Result<Phys, Error> {
    let (file, server) = File::with_disp(disp);
    let dir = dir.borrow();
    dir.open(
        path.as_ref().into(),
        OpenOptions::READ,
        server.try_into().unwrap(),
    )
    .await??;
    file.phys(PhysOptions::Copy).await?
}

pub async fn get_object<D: Borrow<DirectoryClient>, P: AsRef<Path>>(
    disp: &DispSender,
    dir: impl Iterator<Item = D>,
    path: P,
) -> Option<Phys> {
    let path = path.as_ref();
    for dir in dir {
        match get_object_from_dir(disp.clone(), dir, path).await {
            Ok(phys) => return Some(phys),
            Err(err) => log::warn!("Failed to get object from {path:?}: {err}"),
        }
    }
    None
}

pub async fn serve<D: Borrow<DirectoryClient>>(
    disp: DispSender,
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
                let disp = disp.clone();
                let fut = async move {
                    let mut ret = Vec::new();
                    for (index, path) in path.into_iter().enumerate() {
                        match get_object(&disp, dir.clone(), OsStr::from_bytes(path.as_bytes()))
                            .await
                        {
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
