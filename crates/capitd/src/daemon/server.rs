// Author: Dustin Pilgrim
// License: MIT

use capit_ipc::{IpcServer, Result};
use eventline::{debug, info, warn};

use crate::{config, selection::SelectionState, wayland_outputs};
use crate::config::CapitConfig;

use super::handlers::handle_request;
use super::paths::{default_socket_path, ensure_parent_dir, output_dir_from_cfg};
use super::state::{DaemonState, UiCfg};

use std::fs::OpenOptions;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

fn try_acquire_single_instance_lock(lock_path: &Path) -> std::io::Result<Option<std::fs::File>> {
    let f = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(lock_path)?;

    let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };

    if rc == 0 {
        Ok(Some(f))
    } else {
        let e = std::io::Error::last_os_error();
        match e.raw_os_error() {
            Some(libc::EWOULDBLOCK) | Some(libc::EAGAIN) => Ok(None),
            _ => Err(e),
        }
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

fn lock_path_for_socket(sock: &Path) -> PathBuf {
    sock.with_extension("lock")
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
    let lock_path = lock_path_for_socket(&sock);

    let _lock = match try_acquire_single_instance_lock(&lock_path) {
        Ok(Some(f)) => {
            debug!("acquired singleton lock: {}", lock_path.display());
            f
        }
        Ok(None) => {
            let msg = "another instance of capitd is already running.";

            // Always log internally
            warn!("{msg}");

            // Only manually print if NOT verbose
            if !verbose {
                eprintln!("capitd: {msg}");
            }

            return Ok(()); // clean exit
        }
        Err(e) => {
            warn!("failed to acquire lock '{}': {e}", lock_path.display());
            return Err(e.into());
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
    info!("config screenshot_directory={}", state.cfg.screenshot_directory.display());
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
