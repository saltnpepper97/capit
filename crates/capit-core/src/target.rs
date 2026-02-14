// Author: Dustin Pilgrim
// License: MIT

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Target {
    /// Whole desktop / all outputs (combined space).
    AllScreens,

    /// Prefer stable output name if available (e.g. "DP-1", "HDMI-A-1").
    OutputName(String),

    /// Fallback when name isn't known; index is whatever the daemon reports.
    OutputIndex(u32),
}
