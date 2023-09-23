use crate::tcp::TcpSession;
use crate::tokiort::*;
use crate::Handle;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{watch, Semaphore};
use tokio_rustls::TlsAcceptor;
use tracing::Instrument;
use tracing::{error, info, span, Level};

#[derive(Debug)]
pub enum Error {
    Bind(std::io::Error),
    LocalAddr(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Bind(_) => write!(f, "TcpListener::bind() failed"),
            Error::LocalAddr(_) => write!(f, "TcpListener::local_addr() failed"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Bind(source) => Some(source),
            Error::LocalAddr(source) => Some(source),
        }
    }
}

#[derive(Clone)]
pub struct Server<S> {
    pub(crate) socket_addr: SocketAddr,
    pub(crate) max_conns: usize,
    pub(crate) conn_semaphore: Arc<Semaphore>,
    pub(crate) tls: Option<TlsAcceptor>,
    pub(crate) http2: Arc<hyper::server::conn::http2::Builder<TokioExecutor>>,
    pub(crate) service: S,
}

impl<S, B> Server<S>
where
    S: hyper::service::HttpService<hyper::body::Incoming, ResBody = B> + Clone + Send + 'static,
    S::Future: Send,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    B: http_body::Body + Send + 'static,
    B::Data: Send + Sync,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    pub async fn run(self) -> Result<Handle, Error> {
        let tcp_listener = TcpListener::bind(self.socket_addr)
            .await
            .map_err(Error::Bind)?;
        let listen_on = tcp_listener.local_addr().map_err(Error::LocalAddr)?;
        info!(%listen_on, "h2d::Server started");

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let shutdown_tx = Arc::new(shutdown_tx);

        let join_handle = tokio::task::spawn(self.tcp_accept_loop(
            tcp_listener,
            shutdown_tx.clone(),
            shutdown_rx,
        ));

        Ok(Handle {
            shutdown_tx,
            join_handle,
        })
    }

    async fn tcp_accept_loop(
        self,
        tcp_listener: TcpListener,
        shutdown_tx: Arc<watch::Sender<bool>>,
        mut shutdown_rx: watch::Receiver<bool>,
    ) {
        let max_conns = self.max_conns;
        info!(%max_conns, "tcp accept loop started");

        loop {
            let accept = tokio::select! {
                biased;
                _ = shutdown_rx.wait_for(|v| *v) => break,
                r = tcp_listener.accept() => r,
            };

            match accept {
                Err(err) => error!(%err, "tcp_accept"),
                Ok((mut tcp_stream, peer_addr)) => {
                    // let peer_port = peer_addr.port();
                    // let peer_addr = peer_addr.ip();

                    let conn_semaphore = self.conn_semaphore.clone();
                    let permit = match conn_semaphore.clone().try_acquire_owned() {
                        Ok(permit) => {
                            let current_conns = max_conns - conn_semaphore.available_permits();
                            info!(%current_conns, %max_conns, %peer_addr, "tcp_accept");
                            permit
                        }
                        Err(_err) => {
                            error!(%peer_addr, "max_conns over");
                            use tokio::io::AsyncWriteExt;
                            if let Err(err) = tcp_stream.shutdown().await {
                                error!(%err, %peer_addr, "failed to tcp_stream.shutdown()");
                            }
                            continue;
                        }
                    };

                    let tls = self.tls.clone();
                    let http2 = self.http2.clone();
                    let service = self.service.clone();

                    let tcp_sess = TcpSession {
                        shutdown_rx: shutdown_tx.subscribe(),
                        permit,
                        tls,
                        http2,
                        service,
                        tcp_stream,
                    };

                    tokio::task::spawn(
                        async move {
                            tcp_sess.begin().await;
                            info!("tcp session finished");
                        }
                        .instrument(span!(Level::ERROR, "tcp_session", %peer_addr)),
                    );
                }
            }
        }

        info!(%max_conns, "tcp accept loop finished");

        info!("waiting shutdown...");

        _ = self
            .conn_semaphore
            .acquire_many(self.max_conns as u32)
            .await
            .unwrap();

        info!("shutdown complete");
    }
}
