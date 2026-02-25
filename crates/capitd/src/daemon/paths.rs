// Author: Dustin Pilgrim
// License: MIT

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use capit_ipc::Result;
use crate::config::CapitConfig;

/// Runtime dir for IPC files (socket + lock).
/// Prefers XDG_RUNTIME_DIR, falls back to /tmp.
fn runtime_ipc_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("capit")
}

pub fn default_socket_path() -> PathBuf {
    // Move socket into a subfolder:
    //   $XDG_RUNTIME_DIR/capit/capit.sock
    // (fallback: /tmp/capit/capit.sock)
    runtime_ipc_dir().join("capit.sock")
}

pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

pub fn default_log_path(file: &str) -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("capit").join(file)
}

/// Where Capit should save screenshots.
///
/// Priority:
/// 1) $CAPIT_DIR (if set and non-empty)
/// 2) config capit.screenshot_directory (if set/non-empty)
/// 3) $XDG_RUNTIME_DIR
/// 4) /tmp
pub fn output_dir_from_cfg(cfg: &CapitConfig) -> PathBuf {
    if let Some(v) = std::env::var_os("CAPIT_DIR") {
        let p = PathBuf::from(v);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }

    let p = cfg.screenshot_directory.clone();
    if !p.as_os_str().is_empty() {
        return p;
    }

    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

pub fn default_output_path(cfg: &CapitConfig, ext: &str) -> PathBuf {
    let base = output_dir_from_cfg(cfg);

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    base.join(format!("capit-{ts}.{ext}"))
}
