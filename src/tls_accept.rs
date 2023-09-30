use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;
use tracing::{error, info};

pub(crate) async fn tls_accept(
    shutdown_rx: &mut watch::Receiver<bool>,
    tls_acceptor: TlsAcceptor,
    tcp_stream: TcpStream,
) -> Option<TlsStream<TcpStream>> {
    let accept = tokio::select! {
        biased;
        _ = shutdown_rx.wait_for(|v| *v) => return None,
        r = tokio::time::timeout(Duration::from_secs(10), tls_acceptor.accept(tcp_stream)) => r,
    };

    let tls_stream = match accept {
        Err(_elapsed) => {
            error!("timeout");
            return None;
        }
        Ok(Err(err)) => {
            error!(%err);
            return None;
        }
        Ok(Ok(tls_stream)) => tls_stream,
    };

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

    Some(tls_stream)
}
