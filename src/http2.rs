use crate::tokiort::*;
use hyper::service::HttpService;
use std::sync::Arc;
use tokio::sync::{watch, OwnedSemaphorePermit};
use tracing::{error, info};

pub(crate) struct Http2<I, S> {
    pub(crate) shutdown_rx: watch::Receiver<bool>,
    pub(crate) permit: OwnedSemaphorePermit,
    pub(crate) http2: Arc<hyper::server::conn::http2::Builder<TokioExecutor>>,
    pub(crate) io: I,
    pub(crate) service: S,
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
    pub async fn serve(self)
    where
        I: hyper::rt::Read + hyper::rt::Write + Unpin + 'static,
    {
        let Self {
            mut shutdown_rx,
            permit,
            http2,
            io,
            service,
        } = self;

        let conn = http2.serve_connection(io, service);

        info!("http2 connection started");

        enum Status {
            Shutdown,
            Finish(Result<(), hyper::Error>),
        }

        let status = tokio::select! {
            biased;
            _ = shutdown_rx.wait_for(|v| *v) => Status::Shutdown,
            r = conn => Status::Finish(r),
        };

        match status {
            Status::Shutdown => {
                // Pin::new(&mut conn).graceful_shutdown();
                // if let Err(err) = conn.await {
                //     error!(%err, "http2 connection finished with error");
                // }
            }
            Status::Finish(Err(err)) => {
                use std::error::Error;
                use std::io::ErrorKind;
                if let Some(err) = err.source() {
                    if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
                        if let ErrorKind::UnexpectedEof = io_err.kind() {
                            info!("http2 connection finished");
                            drop(permit);
                            return;
                        }
                    }
                }
                error!(%err, "http2 connection finished with error");
            }
            Status::Finish(Ok(())) => {
                info!("http2 connection finished");
            }
        }
        drop(permit);
    }
}
