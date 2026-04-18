use thiserror::Error;

#[derive(Debug, Error)]
pub enum DnsexError {
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Addr Parse Error: {0}")]
    AddrParseError(#[from] std::net::AddrParseError),

    #[error("Proto Error: {0}")]
    ProtoError(#[from] hickory_proto::error::ProtoError),

    #[error("Client Error: {0}")]
    ClientError(#[from] hickory_client::error::ClientError),

    #[error("Config Error: {0}")]
    ConfigError(String),

    #[error("Join Error: {0}")]
    JoinError(#[from] tokio::task::JoinError),

    #[error("Argument Errro: {0}")]
    ArgumentError(String),

    #[error("Walkdir Error: {0}")]
    WalkdirError(#[from] walkdir::Error),
}
