// Author: Dustin Pilgrim
// License: MIT

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use capit_core::{Mode, OutputInfo, Target};
use capit_ipc::{Event, IpcServer, Request, Response, Result};
use eventline::{debug, error, info, warn};

use crate::{capture, overlay_region, selection::SelectionState, wayland_outputs};

#[derive(Debug)]
pub struct DaemonArgs {
    pub verbose: bool,
    pub log_file: Option<PathBuf>,
}

#[derive(Debug, Default)]
struct DaemonState {
    active_job: Option<Mode>,
    outputs: Vec<OutputInfo>,
}

pub fn parse_daemon_args() -> DaemonArgs {
    // Minimal parsing without clap for daemon (keeps it simple).
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

pub fn run() -> Result<()> {
    let sock = default_socket_path();
    info!("socket path: {}", sock.display());

    ensure_parent_dir(&sock)?;
    debug!("parent directory ensured");

    let server = IpcServer::bind(&sock)?;
    info!("listening on {}", sock.display());

    info!("querying Wayland outputs...");
    let outputs = wayland_outputs::query_outputs().unwrap_or_else(|e| {
        warn!("output query failed: {e}");
        Vec::new()
    });

    info!("found {} outputs", outputs.len());
    for (i, o) in outputs.iter().enumerate() {
        debug!(
            "output #{}: {:?} @ ({},{}) {}x{} scale={}",
            i, o.name, o.x, o.y, o.width, o.height, o.scale
        );
    }

    let mut state = DaemonState::default();
    state.outputs = outputs;

    loop {
        debug!("waiting for client connection...");
        let mut conn = server.accept()?;
        info!("client connected");

        // Selection is per-connection (UI session), not global daemon state.
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

fn handle_request(
    state: &mut DaemonState,
    selection: &mut SelectionState,
    conn: &mut capit_ipc::ClientConn,
    req: Request,
) -> Response {
    // Handle StartCapture FIRST
    if let Request::StartCapture {
        mode,
        target,
        with_ui,
    } = req
    {
        info!(
            "StartCapture: mode={:?} target={:?} with_ui={}",
            mode, target, with_ui
        );

        // Region + UI overlay (daemon-side)
        if mode == Mode::Region && with_ui {
            state.active_job = Some(Mode::Region);
            let _ = conn.send_event(Event::CaptureStarted { mode: Mode::Region });

            let output = match determine_output(&state.outputs, target) {
                Ok(o) => o,
                Err(message) => {
                    error!("determine_output failed: {}", message);
                    state.active_job = None;
                    let _ = conn.send_event(Event::CaptureFailed {
                        message: message.clone(),
                    });
                    return Response::Error { message };
                }
            };

            return handle_region_overlay_capture(state, conn, output);
        }

        // Other UI modes not implemented yet
        if with_ui {
            return Response::Error {
                message: "UI sessions are started via the bar; bar UI not implemented yet".into(),
            };
        }

        // Headless captures
        return handle_headless_capture(state, conn, mode, target);
    }

    // Handle SetSelection / ConfirmSelection via SelectionState (future UI-driven modes)
    if matches!(req, Request::SetSelection { .. } | Request::ConfirmSelection) {
        if let Some(resp) = selection.handle_request(&req, |ev: Event| {
            debug!("sending event to client: {:?}", ev);
            let _ = conn.send_event(ev);
        }) {
            if matches!(req, Request::ConfirmSelection) {
                if let Some(sel) = selection.take_active() {
                    state.active_job = Some(sel.mode);

                    match sel.mode {
                        Mode::Region => {
                            let rect = match sel.rect {
                                Some(r) => r,
                                None => {
                                    let msg = "no selection rect set".to_string();
                                    let _ = conn.send_event(Event::CaptureFailed {
                                        message: msg.clone(),
                                    });
                                    state.active_job = None;
                                    return Response::Error { message: msg };
                                }
                            };

                            let out_path = default_output_path("png");
                            let result = capture::capture_screen_to_rect(&out_path, &rect);

                            match result {
                                Ok(()) => {
                                    let _ = conn.send_event(Event::CaptureFinished {
                                        path: out_path.display().to_string(),
                                    });
                                    state.active_job = None;
                                }
                                Err(message) => {
                                    let _ = conn.send_event(Event::CaptureFailed {
                                        message: message.clone(),
                                    });
                                    state.active_job = None;
                                    return Response::Error { message };
                                }
                            }
                        }
                        other => {
                            let msg = format!("ConfirmSelection for {other:?} not implemented yet");
                            let _ = conn.send_event(Event::CaptureFailed {
                                message: msg.clone(),
                            });
                            state.active_job = None;
                            return Response::Error { message: msg };
                        }
                    }
                }
            }
            return resp;
        }
    }

    // All other requests
    match req {
        Request::Hello(_) => Response::Ok,

        Request::Status => Response::Status {
            running: true,
            active_job: state.active_job,
        },

        Request::ListOutputs => Response::Outputs {
            outputs: state.outputs.clone(),
        },

        Request::StartCapture { .. } => Response::Error {
            message: "Internal error: StartCapture not handled properly".into(),
        },

        Request::SetSelection { .. } => Response::Error {
            message: "SetSelection without an active UI session".into(),
        },

        Request::ConfirmSelection => Response::Error {
            message: "ConfirmSelection without an active UI session".into(),
        },

        Request::Cancel => {
            state.active_job = None;
            Response::Ok
        }
    }
}

fn handle_region_overlay_capture(
    state: &mut DaemonState,
    conn: &mut capit_ipc::ClientConn,
    output: OutputInfo,
) -> Response {
    match overlay_region::run_region_overlay(output) {
        Ok(Some(rect)) => {
            info!("overlay confirmed: {:?}", rect);

            let out_path = default_output_path("png");
            info!("capturing to: {}", out_path.display());

            match capture::capture_screen_to_rect(&out_path, &rect) {
                Ok(()) => {
                    info!("capture successful");
                    let _ = conn.send_event(Event::CaptureFinished {
                        path: out_path.display().to_string(),
                    });
                    state.active_job = None;
                    Response::Ok
                }
                Err(message) => {
                    error!("capture failed: {}", message);
                    let _ = conn.send_event(Event::CaptureFailed {
                        message: message.clone(),
                    });
                    state.active_job = None;
                    Response::Error { message }
                }
            }
        }
        Ok(None) => {
            info!("overlay cancelled");
            let _ = conn.send_event(Event::CaptureFailed {
                message: "cancelled".into(),
            });
            state.active_job = None;
            Response::Ok
        }
        Err(message) => {
            error!("overlay error: {}", message);
            let _ = conn.send_event(Event::CaptureFailed {
                message: message.clone(),
            });
            state.active_job = None;
            Response::Error { message }
        }
    }
}

fn handle_headless_capture(
    state: &mut DaemonState,
    conn: &mut capit_ipc::ClientConn,
    mode: Mode,
    target: Option<Target>,
) -> Response {
    match mode {
        Mode::Screen => {
            state.active_job = Some(Mode::Screen);
            let _ = conn.send_event(Event::CaptureStarted { mode: Mode::Screen });

            let out_path = default_output_path("png");

            let result: std::result::Result<(), String> = match target {
                None | Some(Target::AllScreens) => capture::capture_screen_to(&out_path),

                Some(Target::OutputName(name)) => match state
                    .outputs
                    .iter()
                    .find(|o| o.name.as_deref() == Some(name.as_str()))
                {
                    Some(out) => {
                        let s = out.scale.max(1);
                        let crop = capture::CaptureCrop {
                            x: out.x * s,
                            y: out.y * s,
                            w: out.width * s,
                            h: out.height * s,
                        };
                        capture::capture_screen_to_crop(&out_path, crop)
                    }
                    None => {
                        let known = state
                            .outputs
                            .iter()
                            .filter_map(|o| o.name.as_deref())
                            .collect::<Vec<_>>()
                            .join(", ");
                        Err(format!("unknown output '{name}'. Try one of: {known}"))
                    }
                },

                Some(other) => Err(format!(
                    "target not supported for screen capture yet: {other:?}"
                )),
            };

            match result {
                Ok(()) => {
                    let _ = conn.send_event(Event::CaptureFinished {
                        path: out_path.display().to_string(),
                    });
                    state.active_job = None;
                    Response::Ok
                }
                Err(message) => {
                    let _ = conn.send_event(Event::CaptureFailed {
                        message: message.clone(),
                    });
                    state.active_job = None;
                    Response::Error { message }
                }
            }
        }

        Mode::Region => Response::Error {
            message: "headless region not supported (use --ui flag or capit region)".into(),
        },

        Mode::Window => Response::Error {
            message: "headless window not supported yet (use --ui flag)".into(),
        },

        Mode::Record => Response::Error {
            message: "record not implemented yet".into(),
        },
    }
}

fn determine_output(
    outputs: &[OutputInfo],
    target: Option<Target>,
) -> std::result::Result<OutputInfo, String> {
    if outputs.is_empty() {
        return Err("no outputs available".into());
    }

    match target {
        None | Some(Target::AllScreens) => Ok(outputs[0].clone()),
        Some(Target::OutputName(name)) => outputs
            .iter()
            .find(|o| o.name.as_deref() == Some(name.as_str()))
            .cloned()
            .ok_or_else(|| {
                let known = outputs
                    .iter()
                    .filter_map(|o| o.name.as_deref())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("unknown output '{name}'. Available: {known}")
            }),
        other => Err(format!("target not supported for region: {other:?}")),
    }
}

fn default_socket_path() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("capit.sock")
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
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

/// For now, write to $XDG_RUNTIME_DIR (or /tmp) with a timestamp.
/// Later we'll move to XDG Pictures/Capit.
fn default_output_path(ext: &str) -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    base.join(format!("capit-{ts}.{ext}"))
}
