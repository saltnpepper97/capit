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

use eventline::{debug, info, warn};

fn default_socket_path() -> PathBuf {
    if let Ok(p) = std::env::var("CAPIT_SOCKET") {
        return PathBuf::from(p);
    }
    if let Ok(rt) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(rt).join("capit.sock");
    }
    PathBuf::from("/tmp").join("capit.sock")
}

/// Ask daemon for UI config (theme/accent) so bar can match.
fn fetch_ui_config(socket: &Path) -> Result<UiConfig, String> {
    let mut client = ipc::connect(socket)?;
    let resp = client.call(Request::GetUiConfig).map_err(|e| format!("{e}"))?;
    match resp {
        Response::UiConfig { cfg } => Ok(cfg),
        other => Err(format!("expected UiConfig response, got: {other:?}")),
    }
}

fn main() -> Result<(), String> {
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

    let ui = match fetch_ui_config(&socket) {
        Ok(cfg) => {
            debug!(
                "bar ui config: accent=0x{:08X} bg=0x{:08X}",
                cfg.accent_colour, cfg.bar_background_colour
            );
            cfg
        }
        Err(e) => {
            warn!("failed to fetch ui config from daemon: {e}");
            UiConfig {
                accent_colour: 0xFF0A_84FF,
                bar_background_colour: 0xFF0F_1115,
            }
        }
    };

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
