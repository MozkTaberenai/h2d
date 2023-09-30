use crate::tcp_accept::TcpAcceptLoop;
use crate::tokiort::*;
use crate::Handle;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{watch, Semaphore};
use tokio_rustls::TlsAcceptor;
use tracing::info;

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

        info!(%listen_on);

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let shutdown_tx = Arc::new(shutdown_tx);

        let tcp_accept_loop = TcpAcceptLoop {
            max_conns: self.max_conns,
            conn_semaphore: self.conn_semaphore.clone(),
            tls: self.tls,
            http2: self.http2,
            service: self.service,
            tcp_listener,
            shutdown_tx: shutdown_tx.clone(),
            shutdown_rx,
        };

        let join_handle = tokio::task::spawn(tcp_accept_loop.run());

        Ok(Handle::new(
            shutdown_tx,
            join_handle,
            self.max_conns,
            self.conn_semaphore,
        ))
    }
}
