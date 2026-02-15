// Author: Dustin Pilgrim
// License: MIT

use capit_core::{Mode, OutputInfo};
use crate::config::CapitConfig;
use capit_ipc::protocol::UiConfig;

#[derive(Debug, Clone, Copy)]
pub struct UiCfg {
    pub accent_colour: u32,        // ARGB 0xAARRGGBB
    pub bar_background_colour: u32 // ARGB 0xAARRGGBB
}

impl Default for UiCfg {
    fn default() -> Self {
        Self {
            accent_colour: 0xFF0A_84FF,
            bar_background_colour: 0xFF0F_1115,
        }
    }
}

#[derive(Debug)]
pub struct DaemonState {
    pub active_job: Option<Mode>,
    pub outputs: Vec<OutputInfo>,
    pub cfg: CapitConfig,
    pub ui: UiCfg,
}

impl Default for DaemonState {
    fn default() -> Self {
        Self {
            active_job: None,
            outputs: Vec::new(),
            cfg: CapitConfig::default(),
            ui: UiCfg::default(),
        }
    }
}

impl UiCfg {
    pub fn to_ipc(self) -> UiConfig {
        UiConfig {
            accent_colour: self.accent_colour,
            bar_background_colour: self.bar_background_colour,
        }
    }
}
