use crate::tcp_session::TcpSession;
use crate::tokiort::*;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{watch, Semaphore};
use tokio_rustls::TlsAcceptor;
use tracing::{error, info};

pub(crate) struct TcpAcceptLoop<S> {
    pub max_conns: usize,
    pub conn_semaphore: Arc<Semaphore>,
    pub tls: Option<TlsAcceptor>,
    pub http2: Arc<hyper::server::conn::http2::Builder<TokioExecutor>>,
    pub service: S,
    pub tcp_listener: TcpListener,
    pub shutdown_tx: Arc<watch::Sender<bool>>,
    pub shutdown_rx: watch::Receiver<bool>,
}

impl<S, B> TcpAcceptLoop<S>
where
    S: hyper::service::HttpService<hyper::body::Incoming, ResBody = B> + Clone + Send + 'static,
    S::Future: Send,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    B: http_body::Body + Send + 'static,
    B::Data: Send + Sync,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    pub async fn run(self) {
        let TcpAcceptLoop {
            max_conns,
            conn_semaphore,
            tls,
            http2,
            service,
            tcp_listener,
            shutdown_tx,
            mut shutdown_rx,
        } = self;

        info!(%max_conns, "start");

        loop {
            let accept = tokio::select! {
                biased;
                _ = shutdown_rx.wait_for(|v| *v) => break,
                r = tcp_listener.accept() => r,
            };

            match accept {
                Err(err) => error!(%err),
                Ok((mut tcp_stream, peer_addr)) => {
                    let conn_semaphore = conn_semaphore.clone();
                    let permit = match conn_semaphore.clone().try_acquire_owned() {
                        Ok(permit) => {
                            let current_conns = max_conns - conn_semaphore.available_permits();
                            info!(%current_conns, %max_conns, %peer_addr);
                            permit
                        }
                        Err(_err) => {
                            error!(%peer_addr, "exceed max_conns");
                            use tokio::io::AsyncWriteExt;
                            if let Err(err) = tcp_stream.shutdown().await {
                                error!(%err, %peer_addr, "failed to tcp_stream.shutdown()");
                            }
                            continue;
                        }
                    };

                    let tls = tls.clone();
                    let http2 = http2.clone();
                    let service = service.clone();

                    let tcp_sess = TcpSession {
                        shutdown_rx: shutdown_tx.subscribe(),
                        permit,
                        tls,
                        http2,
                        service,
                        tcp_stream,
                    };

                    tokio::task::spawn(tcp_sess.run(peer_addr));
                }
            }
        }

        info!(%max_conns, "finished");

        info!("waiting shutdown...");

        _ = conn_semaphore.acquire_many(max_conns as u32).await.unwrap();

        info!("shutdown complete");
    }
}
