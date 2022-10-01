pub const GET_OBJECT: usize = 0x172386ab2733;

#[cfg(feature = "std")]
mod std {
    use alloc::{ffi::CString, vec::Vec};
    use core::{
        pin::Pin,
        task::{ready, Context, Poll},
    };

    use futures::{stream::FusedStream, Future, Stream};
    use solvent::prelude::{Packet, Phys};
    use solvent_async::ipc::Channel;

    use super::*;

    pub trait Loader: AsRef<Channel> + From<Channel> + TryInto<Channel> {
        type GetObject<'a>: Future<Output = Result<Vec<Phys>, usize>> + 'a
        where
            Self: 'a;
        fn get_object(&self, paths: Vec<CString>) -> Self::GetObject<'_>;
    }

    pub struct LoaderClient {
        inner: crate::Client,
    }

    impl LoaderClient {
        pub fn new(channel: Channel) -> Self {
            LoaderClient {
                inner: crate::Client::new(channel),
            }
        }

        #[inline]
        fn from_inner(inner: crate::Client) -> Self {
            LoaderClient { inner }
        }

        pub fn event_receiver(&self) -> Option<LoaderEventReceiver> {
            self.inner
                .event_receiver()
                .map(|inner| LoaderEventReceiver { inner })
        }

        pub async fn get_object(
            &self,
            paths: Vec<CString>,
        ) -> Result<Result<Vec<Phys>, usize>, crate::Error> {
            let mut packet = Default::default();
            crate::packet::serialize(GET_OBJECT, paths, &mut packet)?;
            let packet = self.inner.call(packet).await?;
            crate::packet::deserialize(GET_OBJECT, &packet, None)
        }
    }

    impl Loader for LoaderClient {
        type GetObject<'a> = impl Future<Output = Result<Vec<Phys>, usize>> + 'a;

        fn get_object(&self, paths: Vec<CString>) -> Self::GetObject<'_> {
            async {
                self.get_object(paths)
                    .await
                    .expect("Failed to send request")
            }
        }
    }

    impl AsRef<Channel> for LoaderClient {
        #[inline]
        fn as_ref(&self) -> &Channel {
            self.inner.as_ref()
        }
    }

    impl From<Channel> for LoaderClient {
        #[inline]
        fn from(channel: Channel) -> Self {
            Self::new(channel)
        }
    }

    impl TryFrom<LoaderClient> for Channel {
        type Error = LoaderClient;

        #[inline]
        fn try_from(client: LoaderClient) -> Result<Self, Self::Error> {
            Channel::try_from(client.inner).map_err(|inner| LoaderClient { inner })
        }
    }

    pub struct LoaderEventReceiver {
        inner: crate::EventReceiver,
    }

    impl Stream for LoaderEventReceiver {
        type Item = Result<LoaderEvent, crate::Error>;

        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(
                ready!(Pin::new(&mut self.inner).poll_next(cx))
                    .map(|inner| inner.map(LoaderEvent::Unknown)),
            )
        }
    }

    impl FusedStream for LoaderEventReceiver {
        fn is_terminated(&self) -> bool {
            self.inner.is_terminated()
        }
    }

    pub enum LoaderEvent {
        Unknown(Packet),
    }

    pub struct LoaderServer {
        inner: crate::Server,
    }

    impl LoaderServer {
        pub fn new(channel: Channel) -> Self {
            LoaderServer {
                inner: crate::Server::new(channel),
            }
        }

        #[inline]
        fn from_inner(inner: crate::Server) -> Self {
            LoaderServer { inner }
        }

        pub fn serve(self) -> (LoaderStream, LoaderEventSender) {
            let (stream, es) = self.inner.serve();
            (
                LoaderStream { inner: stream },
                LoaderEventSender { inner: es },
            )
        }
    }

    impl AsRef<Channel> for LoaderServer {
        #[inline]
        fn as_ref(&self) -> &Channel {
            self.inner.as_ref()
        }
    }

    impl From<Channel> for LoaderServer {
        #[inline]
        fn from(channel: Channel) -> Self {
            Self::new(channel)
        }
    }

    impl TryFrom<LoaderServer> for Channel {
        type Error = LoaderServer;

        #[inline]
        fn try_from(server: LoaderServer) -> Result<Self, Self::Error> {
            Channel::try_from(server.inner).map_err(|inner| LoaderServer { inner })
        }
    }

    pub enum LoaderRequest {
        GetObject {
            paths: Vec<CString>,
            responder: LoaderGetObjectResponder,
        },
        Unknown(crate::Request),
    }

    pub struct LoaderStream {
        inner: crate::PacketStream,
    }

    impl Stream for LoaderStream {
        type Item = Result<LoaderRequest, crate::Error>;

        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(
                ready!(Pin::new(&mut self.inner).poll_next(cx)).map(|res| match res {
                    Ok(req) => {
                        let (m, de) = crate::packet::deserialize_metadata(&req.packet)?;
                        match m {
                            GET_OBJECT => {
                                let paths = crate::packet::deserialize_body(de, None)?;
                                let responder = LoaderGetObjectResponder {
                                    inner: req.responder,
                                };
                                Ok(LoaderRequest::GetObject { paths, responder })
                            }
                            _ => Ok(LoaderRequest::Unknown(req)),
                        }
                    }
                    Err(err) => Err(err),
                }),
            )
        }
    }

    impl FusedStream for LoaderStream {
        #[inline]
        fn is_terminated(&self) -> bool {
            self.inner.is_terminated()
        }
    }

    pub struct LoaderEventSender {
        inner: crate::EventSender,
    }

    impl LoaderEventSender {
        #[inline]
        pub fn send_raw(&self, packet: Packet) -> Result<(), crate::Error> {
            self.inner.send(packet)
        }

        #[inline]
        pub fn close(self) {
            self.inner.close()
        }
    }

    pub struct LoaderGetObjectResponder {
        inner: crate::Responder,
    }

    impl LoaderGetObjectResponder {
        pub fn send(self, ret: Result<Vec<Phys>, usize>) -> Result<(), crate::Error> {
            let mut packet = Default::default();
            crate::packet::serialize(GET_OBJECT, ret, &mut packet)?;
            self.inner.send(packet, false)
        }

        #[inline]
        pub fn close(self) {
            self.inner.close()
        }
    }

    pub fn with_disp(disp: solvent_async::disp::DispSender) -> (LoaderClient, LoaderServer) {
        let (client, server) = crate::with_disp(disp);
        (
            LoaderClient::from_inner(client),
            LoaderServer::from_inner(server),
        )
    }

    pub fn channel() -> (LoaderClient, LoaderServer) {
        with_disp(solvent_async::dispatch())
    }
}

#[cfg(feature = "std")]
pub use std::*;
