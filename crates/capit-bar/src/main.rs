// Author: Dustin Pilgrim
// License: MIT

mod bar;
mod capture;
mod ipc;
mod print;

use std::path::{Path, PathBuf};

use capit_core::{Mode, Target};
use capit_ipc::{Request, Response};
use capit_ipc::protocol::UiConfig;

use eventline::{debug, info};

#[derive(Clone)]
struct CliError(String);

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// Make Debug match Display so Rust prints without quotes/escaped newlines.
impl std::fmt::Debug for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for CliError {}

impl From<ipc::ConnectError> for CliError {
    fn from(e: ipc::ConnectError) -> Self {
        CliError(e.to_string())
    }
}

// ✅ This is what you were missing: convert String errors from run_bar/start_capture.
impl From<String> for CliError {
    fn from(s: String) -> Self {
        CliError(s)
    }
}

fn default_socket_path() -> PathBuf {
    if let Ok(p) = std::env::var("CAPIT_SOCKET") {
        return PathBuf::from(p);
    }
    if let Ok(rt) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(rt).join("capit").join("capit.sock");
    }
    PathBuf::from("/tmp").join("capit").join("capit.sock")
}

/// Ask daemon for UI config (theme/accent) so bar can match.
/// If this fails, we fail fast (do not launch bar without daemon).
fn fetch_ui_config(socket: &Path) -> Result<UiConfig, CliError> {
    let mut client = ipc::connect(socket)?;
    let resp = client
        .call(Request::GetUiConfig)
        .map_err(|e| CliError(format!("{e}")))?;

    match resp {
        Response::UiConfig { cfg } => Ok(cfg),
        other => Err(CliError(format!(
            "capit-bar: expected UiConfig response, got: {other:?}"
        ))),
    }
}

fn main() -> Result<(), CliError> {
    // tiny arg parser: allow `--socket /path/to.sock`
    let mut args = std::env::args().skip(1);
    let mut socket: Option<PathBuf> = None;

    while let Some(a) = args.next() {
        match a.as_str() {
            "--socket" => socket = args.next().map(PathBuf::from),
            _ => {}
        }
    }

    let socket = socket.unwrap_or_else(default_socket_path);
    info!("capit-bar using socket: {}", socket.display());

    // Fail fast if daemon isn't reachable / IPC handshake fails.
    let ui = fetch_ui_config(&socket)?;

    debug!(
        "bar ui config: accent=0x{:08X} bg=0x{:08X}",
        ui.accent_colour, ui.bar_background_colour
    );

    loop {
        let picked = bar::run_bar(ui.accent_colour, ui.bar_background_colour)?;
        let Some(mode) = picked else {
            info!("bar cancelled -> exit");
            std::process::exit(2);
        };

        info!("bar selected mode: {:?}", mode);

        let mut client = ipc::connect(&socket)?;

        let target = match mode {
            Mode::Screen => Some(Target::AllScreens),
            _ => None,
        };

        match capture::start_capture(&mut client, mode, target, false)? {
            capture::CaptureOutcome::Finished { path } => {
                println!("saved to: {path}");
                return Ok(());
            }
            capture::CaptureOutcome::Cancelled => {
                info!("capture cancelled -> back to bar");
                continue;
            }
        }
    }
}
