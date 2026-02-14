// Author: Dustin Pilgrim
// License: MIT

mod capture;
mod overlay_region;
mod selection;
mod wayland_outputs;

mod daemon;
mod logging;

fn main() {
    let args = daemon::parse_daemon_args();

    let log_path = args
        .log_file
        .clone()
        .unwrap_or_else(|| daemon::default_log_path("capitd.log"));

    if let Err(e) = logging::init_logging(&log_path, args.verbose) {
        eprintln!("capitd: failed to init logging: {e}");
    }

    eventline::info!("===== CAPITD STARTING =====");

    if let Err(e) = daemon::run() {
        eventline::error!("fatal error: {e}");
        std::process::exit(1);
    }
}
