// Author: Dustin Pilgrim
// License: MIT

mod cli;
mod client;
mod logging;
mod paths;

use clap::Parser;
use eventline::{debug, error, info};

fn main() {
    let args = cli::Args::parse();

    let log_path = args
        .log_file
        .clone()
        .unwrap_or_else(|| paths::default_log_path("capit.log"));

    // Initialize logging FIRST.
    if let Err(e) = logging::init_logging(&log_path, args.verbose) {
        // This is the only acceptable direct print —
        // logging system failed so we have no logger yet.
        eprintln!("capit: failed to init logging: {e}");
        std::process::exit(1);
    }

    debug!("verbose={}", args.verbose);
    info!("capit starting");
    debug!("log file={}", log_path.display());

    if let Err(e) = client::run::run(args) {
        error!("fatal error: {e}");
        std::process::exit(1);
    }
}
