use thiserror::Error;
use tracing::error;

#[derive(Debug, Error)]
pub enum DnsexError {
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Addr Parse Error: {0}")]
    AddrParseError(#[from] std::net::AddrParseError),

    #[error("Proto Error: {0}")]
    ProtoError(#[from] hickory_proto::error::ProtoError)
}
