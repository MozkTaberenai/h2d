use super::*;
use bytes::Bytes;
use http_body_util::Full;
// use hyper::body::Incoming;
use hyper::{Request, Response};
use std::convert::Infallible;
// use std::future::Future;
// use std::pin::Pin;

// #[derive(Debug, Clone)]
// struct Hello;

// impl hyper::service::Service<Request<Incoming>> for Hello {
//     type Response = Response<Full<Bytes>>;
//     type Error = std::convert::Infallible;
//     type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
//     fn call(&self, _req: Request<Incoming>) -> Self::Future {
//         Box::pin(async { Ok(Response::new(Full::new(Bytes::from_static(b"hello")))) })
//     }
// }

#[tokio::test]
async fn test() {
    let hello = hyper::service::service_fn(|_req| async {
        Ok::<_, Infallible>(Response::new(Full::new(Bytes::from_static(b"hello"))))
    });

    let svh = Config::new("[::1]:8080".parse().unwrap())
        .build(hello)
        .run()
        .await
        .unwrap();

    svh.shutdown().await;
}
