use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

#[derive(Debug)]
pub enum Error {
    NoCertificate {
        path: PathBuf,
    },
    NoPrivateKey {
        path: PathBuf,
    },
    Io {
        source: std::io::Error,
        path: PathBuf,
    },
    Rustls {
        source: rustls::Error,
    },
}

impl From<rustls::Error> for Error {
    fn from(source: rustls::Error) -> Self {
        Self::Rustls { source }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoCertificate { path } => write!(f, "No certificates in {}", path.display()),
            Error::NoPrivateKey { path } => write!(f, "No private key in {}", path.display()),
            Error::Io { path, .. } => write!(f, "IO error on {}", path.display()),
            Error::Rustls { .. } => write!(f, "Rustls error"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::NoCertificate { .. } => None,
            Error::NoPrivateKey { .. } => None,
            Error::Io { source, .. } => Some(source),
            Error::Rustls { source } => Some(source),
        }
    }
}

pub fn acceptor(
    pem: impl AsRef<Path>,
    session_storage: Option<Arc<impl rustls::server::StoresServerSessions + 'static>>,
) -> Result<tokio_rustls::TlsAcceptor, Error> {
    let pem = pem.as_ref();
    let pem_buf = std::fs::read(pem).map_err(|source| Error::Io {
        source,
        path: pem.to_owned(),
    })?;

    let mut certs = vec![];
    let mut key = None;
    for item in rustls_pemfile::read_all(&mut &pem_buf[..]).map_err(|source| Error::Io {
        source,
        path: pem.to_owned(),
    })? {
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
        return Err(Error::NoCertificate {
            path: pem.to_owned(),
        });
    }
    if key.is_none() {
        return Err(Error::NoPrivateKey {
            path: pem.to_owned(),
        });
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
    if let Some(session_storage) = session_storage {
        server_config.session_storage = session_storage;
    }

    let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

    Ok(tls_acceptor)
}
