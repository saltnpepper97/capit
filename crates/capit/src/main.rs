// Author: Dustin Pilgrim
// License: MIT

mod bar;
mod cli;
mod client;
mod logging;
mod paths;

use clap::Parser;
use eventline::{error, info};

fn main() {
    let args = cli::Args::parse();

    if args.verbose {
        eprintln!("capit: verbose mode enabled");
    }

    let log_path = args
        .log_file
        .clone()
        .unwrap_or_else(|| paths::default_log_path("capit.log"));

    eprintln!(
        "capit: initializing logging (file: {}, console: {})",
        log_path.display(),
        args.verbose
    );

    if let Err(e) = logging::init_logging(&log_path, args.verbose) {
        eprintln!("capit: failed to init logging: {e}");
    }

    info!("capit starting");

    if let Err(e) = client::run::run(args) {
        error!("error: {e}");
        eprintln!("{e}");
        std::process::exit(1);
    }
}
