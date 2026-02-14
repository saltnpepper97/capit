// Author: Dustin Pilgrim
// License: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CapitError {
    #[error("capture failed")]
    CaptureFailed,
}
