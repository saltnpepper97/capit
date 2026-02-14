// Author: Dustin Pilgrim
// License: MIT

use thiserror::Error;

pub type Result<T> = std::result::Result<T, IpcError>;

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Ser(#[from] postcard::Error),

    #[error("frame too large")]
    FrameTooLarge,

    #[error("version mismatch (client {client}, server {server})")]
    VersionMismatch { client: u32, server: u32 },

    #[error("daemon returned error: {0}")]
    Remote(String),
}
