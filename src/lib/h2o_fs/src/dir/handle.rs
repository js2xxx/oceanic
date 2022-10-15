use futures::StreamExt;
use solvent_core::{path::Path, sync::Arsc};
use solvent_rpc::{
    io::{dir as rpc, Error, OpenOptions, Permission},
    Error as RpcError, Server,
};

use super::{Directory, DirectoryMut};

pub async fn handle<D: Directory>(
    dir: Arsc<D>,
    server: rpc::DirectoryServer,
    options: OpenOptions,
) {
    let (mut requests, _) = server.serve();
    while let Some(request) = requests.next().await {
        let request = match request {
            Ok(request) => request,
            Err(err) => {
                log::warn!("dir RPC receive error: {err}");
                break;
            }
        };
        match handle_request(&dir, request, options).await {
            HandleRequest::Break => break,
            HandleRequest::Next(Err(err)) => log::warn!("dir RPC send error: {err}"),
            HandleRequest::Continue(_) => log::warn!("dir RPC received unknown request"),
            _ => {}
        }
    }
}

pub async fn handle_mut<D: DirectoryMut>(
    dir: Arsc<D>,
    server: rpc::DirectoryServer,
    options: OpenOptions,
) {
    let (mut requests, _) = server.serve();
    while let Some(request) = requests.next().await {
        let request = match request {
            Ok(request) => request,
            Err(err) => {
                log::warn!("dir RPC receive error: {err}");
                break;
            }
        };
        match handle_request_mut(&dir, request, options).await {
            HandleRequest::Break => break,
            HandleRequest::Next(Err(err)) => log::warn!("dir RPC send error: {err}"),
            HandleRequest::Continue(_) => log::warn!("dir RPC received unknown request"),
            _ => {}
        }
    }
}

enum HandleRequest {
    Break,
    Next(Result<(), RpcError>),
    Continue(rpc::DirectoryRequest),
}

async fn handle_request<D: Directory>(
    dir: &Arsc<D>,
    request: rpc::DirectoryRequest,
    options: OpenOptions,
) -> HandleRequest {
    let res = match request {
        rpc::DirectoryRequest::CloneConnection { conn, responder } => {
            match dir.clone().open(Path::new(""), options, conn) {
                Ok(()) => responder.send(()),
                Err(_) => {
                    responder.close();
                    return HandleRequest::Break;
                }
            }
        }
        rpc::DirectoryRequest::CloseConnection { responder } => {
            responder.close();
            return HandleRequest::Break;
        }
        rpc::DirectoryRequest::Metadata { responder } => responder.send(dir.metadata()),
        rpc::DirectoryRequest::NextDirent { last, responder } => responder.send({
            if options.contains(OpenOptions::READ) {
                dir.next_dirent(last).await
            } else {
                Err(Error::PermissionDenied(Permission::READ))
            }
        }),
        rpc::DirectoryRequest::Open {
            path,
            options,
            conn,
            responder,
        } => responder.send(dir.clone().open(&path, options, conn)),
        request => return HandleRequest::Continue(request),
    };
    HandleRequest::Next(res)
}

async fn handle_request_mut<D: DirectoryMut>(
    dir: &Arsc<D>,
    request: rpc::DirectoryRequest,
    options: OpenOptions,
) -> HandleRequest {
    let request = match handle_request(dir, request, options).await {
        HandleRequest::Continue(res) => res,
        hr => return hr,
    };

    let res = match request {
        rpc::DirectoryRequest::Link {
            old,
            new,
            responder,
        } => responder.send({
            if options.contains(OpenOptions::WRITE) {
                dir.link(&old, &new).await
            } else {
                Err(Error::PermissionDenied(Permission::WRITE))
            }
        }),
        rpc::DirectoryRequest::Rename {
            old,
            new,
            responder,
        } => responder.send({
            if options.contains(OpenOptions::WRITE) {
                dir.rename(&old, &new).await
            } else {
                Err(Error::PermissionDenied(Permission::WRITE))
            }
        }),
        rpc::DirectoryRequest::Unlink { path, responder } => responder.send({
            if options.contains(OpenOptions::WRITE) {
                dir.unlink(&path).await
            } else {
                Err(Error::PermissionDenied(Permission::WRITE))
            }
        }),
        request => return HandleRequest::Continue(request),
    };
    HandleRequest::Next(res)
}
