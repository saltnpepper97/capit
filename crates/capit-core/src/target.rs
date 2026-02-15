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

    /// The currently active/focused toplevel window.
    ///
    /// Notes:
    /// - This is intentionally abstract; on Wayland we may implement it via
    ///   compositor/portal interaction (focus, window picker, etc).
    /// - It avoids needing stable window IDs up-front.
    ActiveWindow,
}
