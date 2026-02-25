// Author: Dustin Pilgrim
// License: MIT

use capit_ipc::{IpcServer, Result};
use eventline::{debug, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::{config, selection::SelectionState, wayland_outputs};
use crate::config::CapitConfig;

use super::instance_lock::{InstanceLock, LockError};

use super::handlers::handle_request;
use super::paths::{default_socket_path, ensure_parent_dir, output_dir_from_cfg};
use super::session;
use super::state::{DaemonState, UiCfg};

use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

/// Check if an IpcError is a WouldBlock error (socket has no pending connections)
fn is_would_block(e: &capit_ipc::IpcError) -> bool {
    // IpcError wraps io::Error, check if it's WouldBlock
    match e {
        capit_ipc::IpcError::Io(io_err) => io_err.kind() == std::io::ErrorKind::WouldBlock,
        _ => false,
    }
}

fn cleanup_stale_socket(sock: &Path) -> std::io::Result<()> {
    if !sock.exists() {
        return Ok(());
    }

    let md = std::fs::symlink_metadata(sock)?;
    if md.file_type().is_socket() {
        std::fs::remove_file(sock)?;
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("socket path exists but is not a unix socket: {}", sock.display()),
        ))
    }
}

fn capit_dir_for_log() -> String {
    // Match output_dir_from_cfg() semantics: treat empty as "not set".
    match std::env::var_os("CAPIT_DIR") {
        None => "(not set)".to_string(),
        Some(v) => {
            let p = PathBuf::from(v);
            if p.as_os_str().is_empty() {
                "(not set)".to_string()
            } else {
                p.display().to_string()
            }
        }
    }
}

pub fn run(verbose: bool) -> Result<()> {
    // Verify Wayland session is alive before starting
    if let Err(e) = session::ensure_wayland_alive() {
        warn!("not running in wayland session: {e}");

        // Only manually print if NOT verbose (eventline already logged)
        if !verbose {
            eprintln!("capitd: not running in wayland session: {e}");
        }

        return Ok(()); // clean exit
    }

    // Load config
    let cfg = match config::load() {
        Ok(c) => c,
        Err(e) => {
            warn!("failed to load config (using defaults): {e}");
            CapitConfig::default()
        }
    };

    let ui = UiCfg {
        accent_colour: cfg.accent_colour,
        bar_background_colour: cfg.bar_background_colour,
    };

    let sock = default_socket_path();
    info!("socket path: {}", sock.display());

    ensure_parent_dir(&sock)?;
    debug!("parent directory ensured");

    // ------------------------------
    // SINGLE INSTANCE GUARD
    // ------------------------------
    let _lock = match InstanceLock::acquire_for_socket(&sock) {
        Ok(l) => {
            debug!("acquired singleton lock");
            l
        }
        Err(e @ LockError::AlreadyRunning(_)) => {
            // Always log internally
            warn!("{e}");

            // Only manually print if NOT verbose
            if !verbose {
                eprintln!("capitd: {e}");
            }

            return Ok(()); // clean exit
        }
        Err(e) => {
            warn!("failed to acquire singleton lock: {e}");
            let io_err = std::io::Error::new(std::io::ErrorKind::Other, e);
            return Err(io_err.into());
        }
    };

    if let Err(e) = cleanup_stale_socket(&sock) {
        warn!("failed to cleanup stale socket '{}': {e}", sock.display());
        return Err(e.into());
    }
    // ------------------------------

    info!("accent_colour=0x{:08X}", ui.accent_colour);

    let mut state = DaemonState::default();
    state.cfg = cfg;
    state.ui = ui;

    let out_dir = output_dir_from_cfg(&state.cfg);
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        warn!("failed to create output dir '{}': {e}", out_dir.display());
    }

    info!("CAPIT_DIR={}", capit_dir_for_log());
    info!(
        "config screenshot_directory={}",
        state.cfg.screenshot_directory.display()
    );
    info!("output dir={}", out_dir.display());

    let server = IpcServer::bind(&sock)?;
    info!("listening on {}", sock.display());

    // CRITICAL: Set socket to non-blocking mode
    // This allows us to check the shutdown flag periodically
    server.set_nonblocking(true)?;

    info!("querying Wayland outputs...");
    let outputs = wayland_outputs::query_outputs().unwrap_or_else(|e| {
        warn!("output query failed: {e}");
        Vec::new()
    });

    info!("found {} outputs", outputs.len());
    state.outputs = outputs;

    // ------------------------------
    // SESSION MONITORING
    // ------------------------------
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    session::spawn_wayland_socket_watcher(Arc::clone(&shutdown_flag));
    info!("session watcher started");
    // ------------------------------

    loop {
        // Check shutdown flag before accept
        if shutdown_flag.load(Ordering::Relaxed) {
            info!("shutdown requested by session watcher");
            break;
        }

        debug!("waiting for client connection...");

        let mut conn = match server.accept() {
            Ok(c) => c,
            Err(e) if is_would_block(&e) => {
                // Nothing to accept; keep loop responsive to watcher shutdown.
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => {
                // Other errors: check shutdown flag and log
                if shutdown_flag.load(Ordering::Relaxed) {
                    info!("shutdown during accept");
                    break;
                }
                warn!("accept error: {e}");
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
        };

        info!("client connected");

        let mut selection = SelectionState::new();

        debug!("waiting for Hello message...");
        let first = conn.recv()?;
        debug!("first message: {:?}", first);
        conn.handle_hello(&first)?;

        debug!("entering request loop...");
        while let Ok(req) = conn.recv() {
            // Check shutdown flag even during client connection
            if shutdown_flag.load(Ordering::Relaxed) {
                info!("shutdown requested during client session");
                return Ok(());
            }

            debug!("request: {:?}", req);
            let resp = handle_request(&mut state, &mut selection, &mut conn, req);
            debug!("sending response: {:?}", resp);
            conn.send(resp)?;
        }

        info!("client disconnected");
    }

    info!("daemon shutting down gracefully");
    Ok(())
}
