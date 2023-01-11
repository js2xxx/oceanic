use alloc::vec;

use futures_lite::StreamExt;
use rpc::FileRequest;
use solvent_async::io::Stream;
use solvent_core::{path::Path, sync::Arsc};
use solvent_rpc::{
    io::{file as rpc, Error, OpenOptions, Permission},
    Server,
};

use super::{stream::*, File};
use crate::{dir::EventTokens, entry::Entry, spawn::Spawner};

#[inline]
pub async fn handle<F: File>(
    file: Arsc<F>,
    spawner: Spawner,
    tokens: EventTokens,
    seeker: usize,
    server: rpc::FileServer,
    options: OpenOptions,
) {
    let (requests, _) = server.serve();
    let direct = DirectFile::new(file, seeker);
    handle_impl(direct, spawner, tokens, requests, options).await
}

#[inline]
pub async fn handle_mapped<F: File>(
    file: Arsc<F>,
    spawner: Spawner,
    tokens: EventTokens,
    cache: Stream,
    server: rpc::FileServer,
    options: OpenOptions,
) {
    let (requests, event) = server.serve();
    let stream = StreamFile::new(file, cache, event);
    handle_impl(stream, spawner, tokens, requests, options).await
}

async fn handle_impl<S: StreamIo>(
    mut file: S,
    spawner: Spawner,
    tokens: EventTokens,
    mut requests: rpc::FileStream,
    options: OpenOptions,
) {
    while let Some(request) = requests.next().await {
        let request = match request {
            Ok(request) => request,
            Err(err) => {
                log::warn!("file RPC receive error: {err}");
                break;
            }
        };
        let res = match request {
            FileRequest::CloneConnection { conn, responder } => {
                let file = Arsc::clone(file.as_file());
                match file.open(
                    spawner.clone(),
                    tokens.clone(),
                    Path::new(""),
                    options,
                    conn,
                ) {
                    Ok(_) => responder.send(()),
                    Err(_) => {
                        responder.close();
                        break;
                    }
                }
            }
            FileRequest::CloseConnection { responder } => {
                responder.close();
                break;
            }
            FileRequest::Flush { responder } => responder.send(file.as_file().flush().await),
            FileRequest::Lock { responder } => responder.send({
                let res = file.lock(spawner.dispatch()).await;
                res.map(|stream| stream.map(Stream::into_raw).ok_or(()))
            }),
            FileRequest::Metadata { responder } => responder.send(file.as_file().metadata()),
            FileRequest::Open {
                path,
                options,
                conn,
                responder,
            } => {
                let file = Arsc::clone(file.as_file());
                responder.send(
                    file.open(spawner.clone(), tokens.clone(), &path, options, conn)
                        .map(drop),
                )
            }
            FileRequest::Read { len, responder } => responder.send({
                if !options.contains(OpenOptions::READ) {
                    Err(Error::PermissionDenied(Permission::READ))
                } else {
                    let mut buf = vec![0; len];
                    let res = file.read(&mut buf).await;
                    res.map(|len| {
                        buf.truncate(len);
                        buf
                    })
                }
            }),
            FileRequest::ReadAt {
                offset,
                len,
                responder,
            } => responder.send({
                if !options.contains(OpenOptions::READ) {
                    Err(Error::PermissionDenied(Permission::READ))
                } else {
                    let mut buf = vec![0; len];
                    let res = file.read_at(offset, &mut buf).await;
                    res.map(|len| {
                        buf.truncate(len);
                        buf
                    })
                }
            }),
            FileRequest::Resize { new_len, responder } => {
                responder.send(if !options.contains(OpenOptions::WRITE) {
                    Err(Error::PermissionDenied(Permission::WRITE))
                } else {
                    file.resize(new_len).await
                })
            }
            FileRequest::Seek { pos, responder } => responder.send(file.seek(pos).await),
            FileRequest::Write { buf, responder } => {
                responder.send(if !options.contains(OpenOptions::WRITE) {
                    Err(Error::PermissionDenied(Permission::WRITE))
                } else {
                    file.write(&buf).await
                })
            }
            FileRequest::WriteAt {
                offset,
                buf,
                responder,
            } => responder.send(if !options.contains(OpenOptions::WRITE) {
                Err(Error::PermissionDenied(Permission::WRITE))
            } else {
                file.write_at(offset, &buf).await
            }),
            FileRequest::Unknown(_) => {
                log::warn!("file RPC received unknown request");
                break;
            }
            FileRequest::Phys { options, responder } => responder.send(file.phys(options).await),
        };

        if let Err(err) = res {
            log::warn!("file RPC send error: {err}");
            break;
        }
    }
}
