use event_listener::EventListener;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::{sleep, Sleep};
use tokio_rustls::server::TlsStream;
use tokio_rustls::Accept;
use tracing::{error, info};

pin_project_lite::pin_project! {
    pub(crate) struct TlsAccept<'a> {
        shutdown: Pin<&'a mut EventListener>,
        #[pin]
        sleep: Sleep,
        #[pin]
        accept: Accept<TcpStream>,
    }
}

impl<'a> TlsAccept<'a> {
    pub fn new(shutdown: Pin<&'a mut EventListener>, accept: Accept<TcpStream>) -> Self {
        Self {
            shutdown,
            sleep: sleep(Duration::from_secs(10)),
            accept,
        }
    }
}

impl Future for TlsAccept<'_> {
    type Output = Option<TlsStream<TcpStream>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let proj = self.project();

        if proj.shutdown.as_mut().poll(cx).is_ready() {
            return Poll::Ready(None);
        }

        if proj.sleep.poll(cx).is_ready() {
            error!("timeout");
            return Poll::Ready(None);
        }

        match proj.accept.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(err)) => {
                error!(%err);
                Poll::Ready(None)
            }
            Poll::Ready(Ok(tls_stream)) => {
                let conn = tls_stream.get_ref().1;
                let server_name = conn.server_name().unwrap_or_default();
                let version = conn
                    .protocol_version()
                    .map(|p| p.as_str().unwrap_or_default())
                    .unwrap_or_default();
                let cipher_suite = conn
                    .negotiated_cipher_suite()
                    .map(|c| c.suite().as_str().unwrap_or_default())
                    .unwrap_or_default();
                info!(
                    %server_name,
                    %version,
                    %cipher_suite,
                );
                Poll::Ready(Some(tls_stream))
            }
        }
    }
}
