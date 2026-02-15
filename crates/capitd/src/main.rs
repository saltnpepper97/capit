// Author: Dustin Pilgrim
// License: MIT

mod capture;
mod config;
mod overlay_region;
mod overlay_screen;
mod portal_window;
mod selection;
mod wayland_outputs;
mod daemon;
mod logging;

use std::path::PathBuf;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "capitd", version, about = "Capit daemon — screenshot capture daemon")]
struct Args {
    /// Log to stderr (in addition to the log file)
    #[arg(short = 'v', long)]
    verbose: bool,

    /// Override log file path (default: $XDG_STATE_HOME/capit/capitd.log)
    #[arg(long)]
    log_file: Option<PathBuf>,
}

fn effective_output_dir() -> PathBuf {
    // Keep this logic identical to daemon.rs's output_dir_from_cfg() helper.
    //
    // Priority:
    // 1) $CAPIT_DIR (if set and non-empty)
    // 2) config capit.screenshot_directory (if set/non-empty)
    // 3) $XDG_RUNTIME_DIR
    // 4) /tmp

    if let Some(v) = std::env::var_os("CAPIT_DIR") {
        let p = PathBuf::from(v);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }

    if let Ok(cfg) = config::load() {
        let p = cfg.screenshot_directory;
        if !p.as_os_str().is_empty() {
            return p;
        }
    }

    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn main() {
    // Parse args FIRST
    let args = Args::parse();

    // Diagnostic: confirm what we parsed
    eprintln!("capitd: args parsed - verbose={:?}", args.verbose);

    let log_path = args
        .log_file
        .unwrap_or_else(|| daemon::default_log_path("capitd.log"));

    // Diagnostic: show what we're about to do
    eprintln!("capitd: log_path={}", log_path.display());
    eprintln!("capitd: calling init_logging with verbose={}", args.verbose);

    if let Err(e) = logging::init_logging(&log_path, args.verbose) {
        eprintln!("capitd: FAILED to init logging: {e}");
        std::process::exit(1);
    }

    eprintln!("capitd: logging initialized successfully");

    // Log CAPIT_DIR + effective directory once (daemon can still validate too if you want)
    let cap_dir_env = std::env::var_os("CAPIT_DIR")
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_else(|| "(not set)".to_string());

    let out_dir = effective_output_dir();
    eventline::info!("CAPIT_DIR={}", cap_dir_env);
    eventline::info!("output dir={}", out_dir.display());

    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        // Non-fatal for now: daemon will still attempt saves and may fallback
        eventline::warn!(
            "failed to create output dir '{}': {e}",
            out_dir.display()
        );
    }

    eventline::info!("===== CAPITD STARTING =====");
    eventline::debug!("verbose={}", args.verbose);

    if let Err(e) = daemon::run() {
        eventline::error!("fatal error: {e}");
        eprintln!("capitd: {e}");
        std::process::exit(1);
    }
}
