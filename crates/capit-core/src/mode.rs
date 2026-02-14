// Author: Dustin Pilgrim
// License: MIT

use serde::{Deserialize, Serialize};

#[cfg(feature = "clap")]
use clap::ValueEnum;

#[cfg_attr(feature = "clap", derive(ValueEnum))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Mode {
    Region,
    Screen,
    Window,
    Record, // future
}
