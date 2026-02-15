// Author: Dustin Pilgrim
// License: MIT

use std::path::Path;

use capit_core::{Mode, Target};
use capit_ipc::{Request, Response};
use capit_ipc::protocol::UiConfig;

use eventline::{debug, error, info};

use crate::bar;
use crate::cli::{self, Args, Cmd};
use crate::paths;

use super::{capture, ipc, print};

pub fn run(args: Args) -> Result<(), String> {
    info!("starting client");
    debug!("parsed args: {:?}", args.cmd);

    let socket = args.socket.unwrap_or_else(paths::default_socket_path);
    debug!("socket: {}", socket.display());

    match args.cmd {
        Cmd::Bar { .. } => run_bar_loop(&socket),

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

/// Ask daemon for UI config (theme/accent) so bar can match.
fn fetch_ui_config(socket: &Path) -> Result<UiConfig, String> {
    let mut client = ipc::connect(socket)?;
    let resp = client.call(Request::GetUiConfig).map_err(|e| format!("{e}"))?;
    match resp {
        Response::UiConfig { cfg } => Ok(cfg),
        other => Err(format!("expected UiConfig response, got: {other:?}")),
    }
}

/// Runs: Bar -> Capture -> (Cancelled => Bar again) / (Finished => print and exit)
fn run_bar_loop(socket: &Path) -> Result<(), String> {
    info!("launching bar loop");

    let ui = match fetch_ui_config(socket) {
        Ok(cfg) => {
            info!(
                "bar ui config: accent=0x{:08X} bg=0x{:08X}",
                cfg.accent_colour, cfg.bar_background_colour
            );
            cfg
        }
        Err(e) => {
            eventline::warn!("failed to fetch ui config from daemon: {e}");
            UiConfig {
                accent_colour: 0xFF0A_84FF,
                bar_background_colour: 0xFF0F_1115,
            }
        }
    };

    loop {
        let picked = bar::run_bar(ui.accent_colour, ui.bar_background_colour)?;
        let Some(mode) = picked else {
            info!("bar cancelled -> exit");
            return Ok(());
        };

        info!("bar selected mode: {:?}", mode);

        let mut client = ipc::connect(socket).map_err(|e| {
            error!("failed to connect to daemon: {e}");
            e
        })?;

        let target = match mode {
            Mode::Screen => Some(Target::AllScreens),
            _ => None,
        };

        match capture::start_capture(&mut client, mode, target, false)? {
            capture::CaptureOutcome::Finished { path } => {
                println!("saved to: {path}");
                return Ok(());
            }
            capture::CaptureOutcome::Cancelled => {
                info!("capture cancelled -> back to bar");
                continue;
            }
        }
    }
}
