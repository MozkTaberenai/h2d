use crate::http2::Http2;
use crate::tls::TlsAccept;
use event_listener::{Event, EventListener};
use hyper_util::rt::{TokioExecutor, TokioIo};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio_rustls::TlsAcceptor;
use tracing::{error, info};

pub(crate) struct TcpAcceptLoop<S> {
    pub max_conns: usize,
    pub conn_semaphore: Arc<Semaphore>,
    pub tls: Option<TlsAcceptor>,
    pub http2: Arc<hyper::server::conn::http2::Builder<TokioExecutor>>,
    pub service: S,
    pub tcp_listener: TcpListener,
    pub shutdown: Arc<Event>,
    pub shutdown_listener: Pin<Box<EventListener>>,
}

impl<S, B> Future for TcpAcceptLoop<S>
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
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let max_conns = self.max_conns;
        loop {
            if self.shutdown_listener.as_mut().poll(cx).is_ready() {
                info!(%max_conns, "finished");
                break Poll::Ready(());
            }

            match self.tcp_listener.poll_accept(cx) {
                Poll::Pending => break Poll::Pending,
                Poll::Ready(Err(err)) => error!(%err),
                Poll::Ready(Ok((mut tcp_stream, peer_addr))) => {
                    let conn_semaphore = self.conn_semaphore.clone();

                    let permit = match conn_semaphore.clone().try_acquire_owned() {
                        Err(_err) => {
                            error!(%peer_addr, "exceed max_conns");
                            tokio::spawn(async move {
                                use tokio::io::AsyncWriteExt;
                                if let Err(err) = tcp_stream.shutdown().await {
                                    error!(%err, %peer_addr, "failed to tcp_stream.shutdown()");
                                }
                            });
                            continue;
                        }
                        Ok(permit) => {
                            let current_conns = max_conns - conn_semaphore.available_permits();
                            info!(%current_conns, %max_conns, %peer_addr);
                            permit
                        }
                    };

                    let mut shutdown = Box::pin(self.shutdown.listen());

                    match self.tls {
                        None => {
                            tokio::spawn(Http2::new(
                                peer_addr,
                                permit,
                                shutdown,
                                self.http2.clone(),
                                TokioIo::new(tcp_stream),
                                self.service.clone(),
                            ));
                        }
                        Some(ref tls_acceptor) => {
                            let accept = tls_acceptor.accept(tcp_stream);
                            let http2 = self.http2.clone();
                            let service = self.service.clone();
                            tokio::spawn(async move {
                                let accept = TlsAccept::new(shutdown.as_mut(), accept);
                                if let Some(tls_stream) = accept.await {
                                    tokio::spawn(Http2::new(
                                        peer_addr,
                                        permit,
                                        shutdown,
                                        http2,
                                        TokioIo::new(tls_stream),
                                        service,
                                    ));
                                }
                            });
                        }
                    }
                }
            }
        }
    }
}
