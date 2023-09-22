use crate::http2::Http2;
use crate::tokiort::*;
use hyper::service::HttpService;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{watch, OwnedSemaphorePermit};
use tokio_rustls::TlsAcceptor;
use tracing::{error, info};

pub(crate) struct TcpSession<S> {
    pub(crate) shutdown_rx: watch::Receiver<bool>,
    pub(crate) permit: OwnedSemaphorePermit,
    pub(crate) tls: Option<TlsAcceptor>,
    pub(crate) http2: Arc<hyper::server::conn::http2::Builder<TokioExecutor>>,
    pub(crate) service: S,
    pub(crate) tcp_stream: TcpStream,
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
    pub async fn begin(self) {
        let Self {
            mut shutdown_rx,
            permit,
            tls,
            http2,
            service,
            tcp_stream,
        } = self;

        match tls {
            None => {
                Http2 {
                    shutdown_rx,
                    permit,
                    http2,
                    io: TokioIo::new(tcp_stream),
                    service,
                }
                .serve()
                .await;
            }
            Some(tls_acceptor) => {
                let accept = tokio::select! {
                    biased;
                    _ = shutdown_rx.wait_for(|v| *v) => return,
                    r = tokio::time::timeout(Duration::from_secs(10), tls_acceptor.accept(tcp_stream)) => r,
                };
                // let accept =
                //     tokio::time::timeout(Duration::from_secs(10), tls_acceptor.accept(tcp_stream))
                //         .await;

                let tls_stream = match accept {
                    Err(_elapsed) => {
                        error!("tls_accept_timeout");
                        return;
                    }
                    Ok(Err(err)) => {
                        error!(%err, "tls_accept");
                        return;
                    }
                    Ok(Ok(tls_stream)) => tls_stream,
                };

                let conn = tls_stream.get_ref().1;
                info!(
                    server_name = ?conn.server_name(),
                    version = ?conn.protocol_version(),
                    cipher_suite = ?conn.negotiated_cipher_suite(),
                    "tls_accept"
                );

                Http2 {
                    shutdown_rx,
                    permit,
                    http2,
                    io: TokioIo::new(tls_stream),
                    service,
                }
                .serve()
                .await;
            }
        }
    }
}
