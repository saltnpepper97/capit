// Author: Dustin Pilgrim
// License: MIT

use capit_ipc::{IpcServer, Result};
use eventline::{debug, info, warn};

use crate::{config, selection::SelectionState, wayland_outputs};
use crate::config::CapitConfig;

use super::handlers::handle_request;
use super::paths::{default_socket_path, ensure_parent_dir, output_dir_from_cfg};
use super::state::{DaemonState, Theme, UiCfg};

pub fn run() -> Result<()> {
    // Load config
    let cfg = match config::load() {
        Ok(c) => c,
        Err(e) => {
            warn!("failed to load config (using defaults): {e}");
            CapitConfig::default()
        }
    };

    // UI settings from config (no magic defaults here; config::default already covers it)
    let ui = UiCfg {
        theme: match cfg.theme {
            config::Theme::Auto => Theme::Auto,
            config::Theme::Dark => Theme::Dark,
            config::Theme::Light => Theme::Light,
        },
        accent_colour: cfg.accent_colour,
        bar_background_colour: cfg.bar_background_colour,
    };

    let sock = default_socket_path();
    info!("socket path: {}", sock.display());

    ensure_parent_dir(&sock)?;
    debug!("parent directory ensured");

    info!("ui theme={:?} accent_colour=0x{:08X}", ui.theme, ui.accent_colour);

    let mut state = DaemonState::default();
    state.cfg = cfg;
    state.ui = ui;

    // Ensure output dir
    let out_dir = output_dir_from_cfg(&state.cfg);
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        warn!("failed to create output dir '{}': {e}", out_dir.display());
    }

    let cap_dir_env = std::env::var_os("CAPIT_DIR")
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_else(|| "(not set)".to_string());
    info!("CAPIT_DIR={}", cap_dir_env);
    info!("output dir={}", out_dir.display());

    let server = IpcServer::bind(&sock)?;
    info!("listening on {}", sock.display());

    info!("querying Wayland outputs...");
    let outputs = wayland_outputs::query_outputs().unwrap_or_else(|e| {
        warn!("output query failed: {e}");
        Vec::new()
    });

    info!("found {} outputs", outputs.len());
    state.outputs = outputs;

    loop {
        debug!("waiting for client connection...");
        let mut conn = server.accept()?;
        info!("client connected");

        let mut selection = SelectionState::new();

        debug!("waiting for Hello message...");
        let first = conn.recv()?;
        debug!("first message: {:?}", first);
        conn.handle_hello(&first)?;

        debug!("entering request loop...");
        while let Ok(req) = conn.recv() {
            debug!("request: {:?}", req);
            let resp = handle_request(&mut state, &mut selection, &mut conn, req);
            debug!("sending response: {:?}", resp);
            conn.send(resp)?;
        }

        info!("client disconnected");
    }
}
