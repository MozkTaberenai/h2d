use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

#[derive(Debug)]
pub enum Error {
    NoCertificate(PathBuf),
    NoPrivateKey(PathBuf),
    Io(std::io::Error, PathBuf),
    Rustls(rustls::Error),
}

impl From<rustls::Error> for Error {
    fn from(err: rustls::Error) -> Self {
        Self::Rustls(err)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoCertificate(path) => write!(f, "no certificate: {}", path.display()),
            Error::NoPrivateKey(path) => write!(f, "no private key: {}", path.display()),
            Error::Io(source, path) => write!(f, "{}: {source:?}", path.display()),
            Error::Rustls(err) => write!(f, "{err:?}"),
        }
    }
}

impl std::error::Error for Error {}

pub fn acceptor(pem: impl AsRef<Path>) -> Result<tokio_rustls::TlsAcceptor, Error> {
    let pem = pem.as_ref();
    let pem_buf = std::fs::read(pem).map_err(|source| Error::Io(source, pem.to_owned()))?;

    let mut certs = vec![];
    let mut key = None;
    for item in rustls_pemfile::read_all(&mut &pem_buf[..])
        .map_err(|source| Error::Io(source, pem.to_owned()))?
    {
        match item {
            rustls_pemfile::Item::X509Certificate(buf) => {
                info!(pem=%pem.display(), "found x509 certificate");
                certs.push(rustls::Certificate(buf));
            }
            rustls_pemfile::Item::RSAKey(buf) => {
                info!(pem=%pem.display(), "found RSA Key");
                key.replace(rustls::PrivateKey(buf));
            }
            rustls_pemfile::Item::PKCS8Key(buf) => {
                info!(pem=%pem.display(), "found PKCS8 Key");
                key.replace(rustls::PrivateKey(buf));
            }
            rustls_pemfile::Item::ECKey(buf) => {
                info!(pem=%pem.display(), "found EC Key");
                key.replace(rustls::PrivateKey(buf));
            }
            _ => {} // skip
        }
    }

    if certs.is_empty() {
        return Err(Error::NoCertificate(pem.to_owned()));
    }
    if key.is_none() {
        return Err(Error::NoPrivateKey(pem.to_owned()));
    }
    let key = key.unwrap();

    let mut server_config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        // .with_cipher_suites(&[
        //     rustls::cipher_suite::TLS13_CHACHA20_POLY1305_SHA256,
        //     // rustls::cipher_suite::TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256,
        //     // rustls::cipher_suite::TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256,
        // ])
        // .with_kx_groups(&[&rustls::kx_group::X25519])
        // .with_protocol_versions(&[
        //     &rustls::version::TLS13,
        //     // &rustls::version::TLS12,
        // ])?
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    server_config.alpn_protocols = vec![b"h2".to_vec()];

    let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

    Ok(tls_acceptor)
}
