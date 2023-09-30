use crate::http2::Http2;
use crate::tls_accept::tls_accept;
use crate::tokiort::*;
use hyper::service::HttpService;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{watch, OwnedSemaphorePermit};
use tokio_rustls::TlsAcceptor;
use tracing::{info, instrument};

pub(crate) struct TcpSession<S> {
    pub shutdown_rx: watch::Receiver<bool>,
    pub permit: OwnedSemaphorePermit,
    pub tls: Option<TlsAcceptor>,
    pub http2: Arc<hyper::server::conn::http2::Builder<TokioExecutor>>,
    pub service: S,
    pub tcp_stream: TcpStream,
}

impl<S, B> TcpSession<S>
where
    S: HttpService<hyper::body::Incoming, ResBody = B> + Clone + Send + 'static,
    S::Future: Send,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    B: http_body::Body + Send + 'static,
    B::Data: Send + Sync,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    #[instrument(name = "tcp_session", skip(self))]
    pub async fn run(self, peer_addr: SocketAddr) {
        let Self {
            mut shutdown_rx,
            permit,
            tls,
            http2,
            service,
            tcp_stream,
        } = self;

        info!("start");

        match tls {
            None => {
                let io = TokioIo::new(tcp_stream);

                Http2 {
                    shutdown_rx,
                    permit,
                    http2,
                    io,
                    service,
                }
                .serve()
                .await;
            }
            Some(tls_acceptor) => {
                let Some(tls_stream) = tls_accept(&mut shutdown_rx, tls_acceptor, tcp_stream).await
                else {
                    return;
                };

                let io = TokioIo::new(tls_stream);

                Http2 {
                    shutdown_rx,
                    permit,
                    http2,
                    io,
                    service,
                }
                .serve()
                .await;
            }
        }

        info!("finish");
    }
}
