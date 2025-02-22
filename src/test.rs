use super::*;
use bytes::Bytes;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Incoming;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tracing::{error, info};

type AnyError = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, AnyError>;

#[tokio::test]
async fn test() {
    if std::env::var("RUST_LOG").is_err() {
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("RUST_LOG", "info") };
    }

    let ansi = std::env::var_os("NO_COLOR").is_none();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_ansi(ansi)
        .without_time()
        .init();

    let hello = hyper::service::service_fn(|_req| async {
        Ok::<_, Infallible>(Response::new(Full::new(Bytes::from_static(b"hello"))))
    });

    let addr = "[::1]:8080".parse().unwrap();

    let svh = Config::new(addr)
        .max_conns(3)
        .build(hello)
        .run()
        .await
        .unwrap();

    // first connection
    let mut send_req1 = connect(addr).await.unwrap();

    // first request
    let (
        http::response::Parts {
            status, version, ..
        },
        body,
    ) = request(&mut send_req1).await.unwrap().into_parts();

    assert!(status.is_success());
    assert_eq!(version, http::Version::HTTP_2);

    let body = body.collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"hello");

    // second request
    let (
        http::response::Parts {
            status, version, ..
        },
        body,
    ) = request(&mut send_req1).await.unwrap().into_parts();

    assert!(status.is_success());
    assert_eq!(version, http::Version::HTTP_2);

    let body = body.collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"hello");

    sleep(1).await;

    // second connection
    let mut send_req2 = connect(addr).await.unwrap();
    assert!(request(&mut send_req2).await.is_ok());

    sleep(1).await;

    // third connection
    let mut send_req3 = connect(addr).await.unwrap();
    assert!(request(&mut send_req3).await.is_ok());

    sleep(1).await;

    let mut send_req4 = connect(addr).await.unwrap();
    assert!(request(&mut send_req4).await.is_err());

    sleep(1).await;

    let mut send_req5 = connect(addr).await.unwrap();
    assert!(request(&mut send_req5).await.is_err());

    drop(send_req2);
    drop(send_req4);
    drop(send_req5);

    sleep(1).await;

    let mut send_req6 = connect(addr).await.unwrap();
    assert!(request(&mut send_req6).await.is_ok());

    info!("start shutdowning server");
    svh.shutdown().await;
}

async fn sleep(secs: u64) {
    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
}

async fn connect(
    addr: SocketAddr,
) -> Result<hyper::client::conn::http2::SendRequest<Empty<Bytes>>> {
    let stream = TcpStream::connect(addr).await?;

    let (send_req, conn) = hyper::client::conn::http2::handshake::<_, _, Empty<Bytes>>(
        TokioExecutor::new(),
        TokioIo::new(stream),
    )
    .await?;

    tokio::task::spawn(async move {
        if let Err(err) = conn.await {
            error!(%err);
        }
    });

    Ok(send_req)
}

async fn request(
    send_req: &mut hyper::client::conn::http2::SendRequest<Empty<Bytes>>,
) -> Result<Response<Incoming>> {
    // send_req.ready().await?;
    Ok(send_req.send_request(Request::new(Empty::new())).await?)
}
