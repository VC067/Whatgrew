pub mod diff;
pub mod model;
pub mod platform;
pub mod rules;
pub mod scan;
pub mod snapshot;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WgError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid snapshot: {0}")]
    Format(String),
    #[error("codec: {0}")]
    Codec(String),
    #[error("scan cancelled")]
    Cancelled,
    #[error("cannot compare snapshots from different roots")]
    DifferentRoots,
}
