use futures_lite::StreamExt;
use solvent_rpc::{
    ddk::driver::{DriverRequest, DriverServer},
    Server,
};

pub async fn handle_driver(server: DriverServer) {
    let (mut stream, _) = server.serve();
    while let Some(request) = stream.next().await {
        let request = match request {
            Ok(request) => request,
            Err(err) => {
                log::warn!("RPC receive error: {err}");
                continue;
            }
        };

        let res = match request {
            DriverRequest::CloseConnection { responder } => responder.send(()),
            DriverRequest::Unknown(_) => {
                log::warn!("unknown request received");
                continue;
            }
        };

        if let Err(err) = res {
            log::warn!("RPC send error: {err}")
        }
    }
}
