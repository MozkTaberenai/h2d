use event_listener::EventListener;
use hyper::service::HttpService;
use hyper::{body::Incoming, server::conn::http2::Connection};
use hyper_util::rt::TokioExecutor;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::OwnedSemaphorePermit;
use tracing::{error, info};

pub(crate) struct Http2<I, S: hyper::service::HttpService<Incoming>> {
    peer_addr: SocketAddr,
    _permit: OwnedSemaphorePermit,
    shutdown: Pin<Box<EventListener>>,
    conn: Connection<I, S, TokioExecutor>,
}

impl<I, S, B> Http2<I, S>
where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + 'static,
    S: HttpService<hyper::body::Incoming, ResBody = B> + Clone + Send + 'static,
    S::Future: Send,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    B: http_body::Body + Send + 'static,
    B::Data: Send + Sync,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    pub fn new(
        peer_addr: SocketAddr,
        permit: OwnedSemaphorePermit,
        shutdown: Pin<Box<EventListener>>,
        http2: Arc<hyper::server::conn::http2::Builder<TokioExecutor>>,
        io: I,
        service: S,
    ) -> Self {
        info!(%peer_addr, "start");
        Self {
            peer_addr,
            shutdown,
            _permit: permit,
            conn: http2.serve_connection(io, service),
        }
    }
}

impl<I, S, B> Future for Http2<I, S>
where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + 'static,
    S: HttpService<hyper::body::Incoming, ResBody = B> + Clone + Send + 'static,
    S::Future: Send,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    B: http_body::Body + Send + 'static,
    B::Data: Send + Sync,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let peer_addr = self.peer_addr;

        if self.shutdown.as_mut().poll(cx).is_ready() {
            return Poll::Ready(());
        }

        match Pin::new(&mut self.conn).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(err)) => {
                use std::error::Error;
                use std::io::ErrorKind;
                if let Some(err) = err.source() {
                    if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
                        if let ErrorKind::UnexpectedEof = io_err.kind() {
                            info!(%peer_addr, "finish");
                            return Poll::Ready(());
                        }
                    }
                }
                error!(%err, %peer_addr, "finish");
                Poll::Ready(())
            }
            Poll::Ready(Ok(())) => {
                info!(%peer_addr, "finish");
                Poll::Ready(())
            }
        }
    }
}
