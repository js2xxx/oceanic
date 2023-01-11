use futures_lite::StreamExt;
use solvent::prelude::Handle;
use solvent_core::{path::Path, sync::Arsc};
use solvent_rpc::{
    io::{
        dir::{self as rpc, DirectoryEventSender, EventFlags},
        Error, OpenOptions, Permission,
    },
    Error as RpcError, EventSender, Server,
};

use super::{Directory, DirectoryMut, EventTokens};
use crate::spawn::Spawner;

pub async fn handle<D: Directory>(
    dir: Arsc<D>,
    spawner: Spawner,
    tokens: EventTokens,
    server: rpc::DirectoryServer,
    options: OpenOptions,
) {
    let (mut requests, event) = server.serve();
    while let Some(request) = requests.next().await {
        let request = match request {
            Ok(request) => request,
            Err(err) => {
                log::warn!("dir RPC receive error: {err}");
                break;
            }
        };
        match handle_request(&dir, spawner.clone(), &tokens, request, options, &event).await {
            HandleRequest::Break => break,
            HandleRequest::Next(Err(err)) => log::warn!("dir RPC send error: {err}"),
            HandleRequest::Continue(_) => log::warn!("dir RPC received unknown request"),
            _ => {}
        }
    }
}

pub async fn handle_mut<D: DirectoryMut>(
    dir: Arsc<D>,
    spawner: Spawner,
    tokens: EventTokens,
    server: rpc::DirectoryServer,
    options: OpenOptions,
) {
    let (mut requests, event) = server.serve();
    let mut handle = None;
    while let Some(request) = requests.next().await {
        let request = match request {
            Ok(request) => request,
            Err(err) => {
                log::warn!("dir RPC receive error: {err}");
                break;
            }
        };
        match handle_request_mut(
            &dir,
            spawner.clone(),
            &tokens,
            request,
            options,
            &event,
            &mut handle,
        )
        .await
        {
            HandleRequest::Break => break,
            HandleRequest::Next(Err(err)) => log::warn!("dir RPC send error: {err}"),
            HandleRequest::Continue(_) => log::warn!("dir RPC received unknown request"),
            _ => {}
        }
    }
    if let Some(handle) = handle {
        tokens.remove(handle).await
    }
}

enum HandleRequest {
    Break,
    Next(Result<(), RpcError>),
    Continue(rpc::DirectoryRequest),
}

async fn handle_request<D: Directory>(
    dir: &Arsc<D>,
    spawner: Spawner,
    tokens: &EventTokens,
    request: rpc::DirectoryRequest,
    options: OpenOptions,
    event: &rpc::DirectoryEventSender,
) -> HandleRequest {
    let res = match request {
        rpc::DirectoryRequest::CloneConnection { conn, responder } => {
            match dir
                .clone()
                .open(spawner, tokens.clone(), Path::new(""), options, conn)
            {
                Ok(_) => responder.send(()),
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
        } => responder.send({
            dir.clone()
                .open(spawner, tokens.clone(), &path, options, conn)
                .map(|create| {
                    if create {
                        let _ = event.send(EventFlags::ADD);
                    }
                })
        }),
        request => return HandleRequest::Continue(request),
    };
    HandleRequest::Next(res)
}

async fn handle_request_mut<D: DirectoryMut>(
    dir: &Arsc<D>,
    spawner: Spawner,
    tokens: &EventTokens,
    request: rpc::DirectoryRequest,
    options: OpenOptions,
    event: &rpc::DirectoryEventSender,
    handle: &mut Option<Handle>,
) -> HandleRequest {
    let request = match handle_request(dir, spawner, tokens, request, options, event).await {
        HandleRequest::Continue(res) => res,
        hr => return hr,
    };

    let res = match request {
        rpc::DirectoryRequest::EventToken { responder } => responder.send({
            if !options.contains(OpenOptions::WRITE) {
                Err(Error::PermissionDenied(Permission::WRITE))
            } else {
                let raw = event.as_raw();
                *handle = Some(raw);
                // SAFETY: `raw` is the raw reference of a `DirectoryEventSender`.
                unsafe { tokens.insert(dir.clone(), raw, options) }.await;
                Ok(raw)
            }
        }),
        rpc::DirectoryRequest::Link {
            src,
            dst_parent,
            dst,
            responder,
        } => responder.send({
            if options.contains(OpenOptions::WRITE) {
                match tokens
                    .take_if(dst_parent, |ent, options| {
                        ent.clone().into_any().downcast::<D>().is_ok()
                            && options.contains(OpenOptions::WRITE)
                    })
                    .await
                {
                    Some(dst_p) => {
                        let res = dir.clone().link(&src, dst_p, &dst).await;
                        res.inspect(|_| unsafe {
                            // SAFETY: The handle is taken from `tokens`.
                            DirectoryEventSender::send_from_raw(dst_parent, EventFlags::ADD)
                        })
                    }
                    None => Err(Error::PermissionDenied(Permission::WRITE)),
                }
            } else {
                Err(Error::PermissionDenied(Permission::WRITE))
            }
        }),
        rpc::DirectoryRequest::Rename {
            src,
            dst_parent,
            dst,
            responder,
        } => responder.send({
            if options.contains(OpenOptions::WRITE) {
                match tokens
                    .take_if(dst_parent, |ent, options| {
                        ent.clone().into_any().downcast::<D>().is_ok()
                            && options.contains(OpenOptions::WRITE)
                    })
                    .await
                {
                    Some(dst_p) => {
                        let res = dir.clone().rename(&src, dst_p, &dst).await;
                        res.inspect(|_| unsafe {
                            let _ = event.send(EventFlags::REMOVE);
                            // SAFETY: The handle is taken from `tokens`.
                            DirectoryEventSender::send_from_raw(dst_parent, EventFlags::ADD)
                        })
                    }
                    None => Err(Error::PermissionDenied(Permission::WRITE)),
                }
            } else {
                Err(Error::PermissionDenied(Permission::WRITE))
            }
        }),
        rpc::DirectoryRequest::Unlink {
            name,
            expect_dir,
            responder,
        } => responder.send({
            if options.contains(OpenOptions::WRITE) {
                dir.unlink(&name, expect_dir)
                    .await
                    .inspect(|_| drop(event.send(EventFlags::REMOVE)))
            } else {
                Err(Error::PermissionDenied(Permission::WRITE))
            }
        }),
        request => return HandleRequest::Continue(request),
    };
    HandleRequest::Next(res)
}
