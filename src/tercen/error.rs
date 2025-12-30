use thiserror::Error;

/// Errors that can occur when interacting with Tercen services
#[derive(Debug, Error)]
pub enum TercenError {
    /// gRPC transport or protocol error
    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::Status),

    /// gRPC transport error
    #[error("Transport error: {0}")]
    Transport(#[from] tonic::transport::Error),

    /// Authentication error
    #[error("Authentication error: {0}")]
    Auth(String),

    /// Configuration error (missing env vars, invalid URIs, etc.)
    #[error("Configuration error: {0}")]
    Config(String),

    /// Connection error
    #[error("Connection error: {0}")]
    Connection(String),

    /// Generic error
    #[error("{0}")]
    Other(String),
}

/// Type alias for Results using TercenError
pub type Result<T> = std::result::Result<T, TercenError>;
