use crate::tcp::TcpAcceptLoop;
use crate::tokiort::*;
use crate::Handle;
use event_listener::Event;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
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
    S: hyper::service::HttpService<hyper::body::Incoming, ResBody = B>
        + Clone
        + Send
        + 'static
        + Unpin,
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

        let shutdown = Arc::new(Event::new());
        let shutdown_listener = shutdown.listen();

        let tcp_accept_loop = TcpAcceptLoop {
            max_conns: self.max_conns,
            conn_semaphore: self.conn_semaphore.clone(),
            tls: self.tls,
            http2: self.http2,
            service: self.service,
            tcp_listener,
            shutdown: shutdown.clone(),
            shutdown_listener,
        };

        let max_conns = self.max_conns;
        let conn_semaphore = self.conn_semaphore.clone();
        let join_handle = tokio::spawn(async move {
            tcp_accept_loop.await;
            info!("waiting shutdown...");
            _ = conn_semaphore.acquire_many(max_conns as u32).await.unwrap();
            info!("shutdown complete");
        });

        Ok(Handle::new(
            shutdown,
            join_handle,
            self.max_conns,
            self.conn_semaphore,
        ))
    }
}
