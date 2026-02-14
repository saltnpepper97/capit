// Author: Dustin Pilgrim
// License: MIT

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputInfo {
    /// Compositor-provided name when available (wlroots often has this).
    pub name: Option<String>,

    /// Logical position in the global desktop space.
    pub x: i32,
    pub y: i32,

    /// Logical size (not physical pixels).
    pub width: i32,
    pub height: i32,

    /// Scale factor (e.g. 1, 2). Keep as i32 for simplicity.
    pub scale: i32,
}
