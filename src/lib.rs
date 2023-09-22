mod config;
mod error;
mod handle;
mod http2;
mod server;
mod tcp;
mod tls;

// from: https://github.com/hyperium/hyper/blob/master/benches/support/tokiort.rs
// rev: f9f65b7
mod tokiort;
// ToDo: use https://github.com/hyperium/hyper-util

pub use crate::config::Config;
pub use crate::error::Error;
pub use crate::handle::Handle;
pub use crate::server::Server;

// ToDo: AnyError -> Owned Error type
pub type AnyError = Box<dyn std::error::Error + Send + Sync>;
type Result<T, E = AnyError> = std::result::Result<T, E>;

#[cfg(test)]
mod test;
