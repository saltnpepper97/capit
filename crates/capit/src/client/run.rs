// Author: Dustin Pilgrim
// License: MIT

use std::path::Path;

use capit_core::{Mode, Target};
use capit_ipc::Request;

use eventline::{debug, info};

use crate::cli::{self, Args, Cmd};
use crate::paths;

use super::{capture, ipc, print};

pub fn run(args: Args) -> Result<(), String> {
    info!("starting client");
    debug!("parsed args: {:?}", args.cmd);

    let socket = args.socket.unwrap_or_else(paths::default_socket_path);
    debug!("socket: {}", socket.display());

    match args.cmd {
        Cmd::Bar { .. } => run_capit_bar(&socket),

        _ => {
            let mut client = ipc::connect(&socket)?;
            info!("connected to daemon");

            match args.cmd {
                Cmd::Status => {
                    let resp = client.call(Request::Status).map_err(|e| format!("{e}"))?;
                    print::print_response(resp);
                    Ok(())
                }

                Cmd::Outputs => {
                    let resp = client.call(Request::ListOutputs).map_err(|e| format!("{e}"))?;
                    print::print_outputs_or_fallback(resp);
                    Ok(())
                }

                Cmd::Cancel => {
                    let resp = client.call(Request::Cancel).map_err(|e| format!("{e}"))?;
                    print::print_response(resp);
                    Ok(())
                }

                Cmd::Region { output } => {
                    let target = cli::target_from_output_name(output);

                    match capture::start_capture(&mut client, Mode::Region, target, false)? {
                        capture::CaptureOutcome::Finished { path } => {
                            println!("saved to: {path}");
                            Ok(())
                        }
                        capture::CaptureOutcome::Cancelled => {
                            info!("capture cancelled");
                            Ok(())
                        }
                    }
                }

                Cmd::Screen { output } => {
                    let target = match output {
                        Some(name) => Some(Target::OutputName(name)),
                        None => Some(Target::AllScreens),
                    };

                    match capture::start_capture(&mut client, Mode::Screen, target, false)? {
                        capture::CaptureOutcome::Finished { path } => {
                            println!("saved to: {path}");
                            Ok(())
                        }
                        capture::CaptureOutcome::Cancelled => {
                            info!("capture cancelled");
                            Ok(())
                        }
                    }
                }

                Cmd::Window => match capture::start_capture(&mut client, Mode::Window, None, false)? {
                    capture::CaptureOutcome::Finished { path } => {
                        println!("saved to: {path}");
                        Ok(())
                    }
                    capture::CaptureOutcome::Cancelled => {
                        info!("capture cancelled");
                        Ok(())
                    }
                },

                Cmd::Bar { .. } => unreachable!(),
            }
        }
    }
}

fn run_capit_bar(socket: &Path) -> Result<(), String> {
    use std::process::Command;

    let status = match Command::new("capit-bar")
        .arg("--socket")
        .arg(socket)
        .status()
    {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(
                "capit-bar is not installed or not in PATH. Build it with: cargo build -p capit-bar"
                    .into(),
            );
        }
        Err(e) => return Err(format!("failed to run capit-bar: {e}")),
    };

    match status.code() {
        Some(0) => Ok(()),
        Some(2) => Ok(()), // cancelled
        Some(c) => Err(format!("capit-bar exited with code {c}")),
        None => Err("capit-bar terminated by signal".into()),
    }
}
