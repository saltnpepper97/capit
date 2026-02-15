// Author: Dustin Pilgrim
// License: MIT

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use capit_core::{Mode, Target};

#[derive(Debug, Parser)]
#[command(name = "capit", version, about = "Capit — capture it.")]
pub struct Args {
    /// Override IPC socket path (default: $XDG_RUNTIME_DIR/capit.sock)
    #[arg(long)]
    pub socket: Option<PathBuf>,

    /// Log to stderr (in addition to the log file)
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Override log file path (default: $XDG_STATE_HOME/capit/capit.log)
    #[arg(long)]
    pub log_file: Option<PathBuf>,

    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Show daemon status
    Status,

    /// List outputs (monitors)
    Outputs,

    /// Cancel active capture job
    Cancel,

    /// Show floating bar UI (lets you pick mode/target/options)
    Bar {
        /// Preselect a mode (screen/region/window/record)
        #[arg(long)]
        mode: Option<Mode>,
        /// Preselect an output name (e.g. DP-1)
        #[arg(long, short = 'o')]
        output: Option<String>,
    },

    /// Start a region capture (mouse-driven overlay)
    Region {
        /// Optionally target a specific output by name
        #[arg(long, short = 'o')]
        output: Option<String>,
    },

    /// Start a full-screen capture (daemon-side overlay)
    Screen {
        /// Capture a specific output by name, otherwise all screens
        #[arg(long, short = 'o')]
        output: Option<String>,
    },

    /// Start a window capture (not implemented yet)
    Window,
}

// handy helpers (keeps run.rs clean)
pub fn target_from_output_name(output: Option<String>) -> Option<Target> {
    output.map(Target::OutputName)
}
