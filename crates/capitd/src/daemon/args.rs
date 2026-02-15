// Author: Dustin Pilgrim
// License: MIT

use std::path::PathBuf;

#[derive(Debug)]
pub struct DaemonArgs {
    pub verbose: bool,
    pub log_file: Option<PathBuf>,
}

pub fn parse_daemon_args() -> DaemonArgs {
    let mut verbose = false;
    let mut log_file: Option<PathBuf> = None;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-v" | "--verbose" => verbose = true,
            "--log-file" => {
                if let Some(p) = it.next() {
                    log_file = Some(PathBuf::from(p));
                }
            }
            _ => {}
        }
    }

    DaemonArgs { verbose, log_file }
}
