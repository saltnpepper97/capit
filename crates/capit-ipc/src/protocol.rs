// Author: Dustin Pilgrim
// License: MIT

use serde::{Deserialize, Serialize};
use capit_core::{Mode, Target, OutputInfo, Rect};

pub const IPC_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Wire {
    Response(Response),
    Event(Event),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcHello {
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Hello(IpcHello),

    /// Query current outputs (monitors) and their layout in global space.
    ListOutputs,

    StartCapture {
        mode: Mode,

        /// Optional output target (primarily for Screen/Record, but usable for others).
        target: Option<Target>,

        /// Lets daemon know if an interactive UI session is active.
        with_ui: bool,
    },

    /// UI → daemon: send the currently selected rectangle (global coords).
    /// Can be sent repeatedly while dragging to drive live preview.
    SetSelection { rect: Rect },

    /// UI → daemon: confirm the current selection (commit capture).
    ConfirmSelection,

    Cancel,
    Status,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Ok,

    /// Response to ListOutputs.
    Outputs { outputs: Vec<OutputInfo> },

    Status {
        running: bool,
        active_job: Option<Mode>,
    },

    Error { message: String },
}

/// Daemon → client async notifications.
/// CLI can mostly ignore these; UI will use them heavily.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    CaptureStarted { mode: Mode },
    CaptureFinished { path: String },
    CaptureFailed { message: String },

    /// Daemon → UI: preview rectangle accepted/normalized (or echoed back).
    /// Useful if daemon snaps/clamps to outputs.
    SelectionPreview { rect: Rect },
}
