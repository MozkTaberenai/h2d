use crate::Server;
use hyper_util::rt::TokioExecutor;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_rustls::TlsAcceptor;

pub struct Config {
    socket_addr: SocketAddr,
    max_conns: usize,
    tls: Option<TlsAcceptor>,
    http2: hyper::server::conn::http2::Builder<TokioExecutor>,
}

impl Config {
    pub fn new(socket_addr: SocketAddr) -> Self {
        Self {
            socket_addr,
            max_conns: 10000,
            tls: None,
            http2: hyper::server::conn::http2::Builder::new(TokioExecutor::new()),
        }
    }

    pub fn max_conns(mut self, max_conns: u32) -> Self {
        let max_conns = max_conns as usize;
        assert!(max_conns <= tokio::sync::Semaphore::MAX_PERMITS);
        self.max_conns = max_conns;
        self
    }

    pub fn http2_conf(
        mut self,
        f: impl FnOnce(&mut hyper::server::conn::http2::Builder<TokioExecutor>),
    ) -> Self {
        f(&mut self.http2);
        self
    }

    // pub fn use_keep_alive(mut self) -> Self {
    //     self.http2
    //         .timer(TokioTimer)
    //         .adaptive_window(true)
    //         .keep_alive_interval(Some(Duration::from_secs(60)));
    //     self
    // }

    pub fn use_tls(mut self, mut server_config: rustls::ServerConfig) -> Self {
        server_config.alpn_protocols = vec![b"h2".to_vec()];
        let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));
        self.tls.replace(tls_acceptor);
        self
    }

    pub fn build<S, B>(self, service: S) -> Server<S>
    where
        S: hyper::service::HttpService<hyper::body::Incoming, ResBody = B> + Clone + Send + 'static,
        S::Future: Send,
        S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        B: http_body::Body + Send + 'static,
        B::Data: Send + Sync,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let max_conns = match self.max_conns {
            0 => tokio::sync::Semaphore::MAX_PERMITS,
            n => n,
        };

        Server {
            socket_addr: self.socket_addr,
            max_conns,
            conn_semaphore: Arc::new(Semaphore::new(max_conns)),
            tls: self.tls,
            http2: Arc::new(self.http2),
            service,
        }
    }
}
