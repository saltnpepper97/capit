// Author: Dustin Pilgrim
// License: MIT

use capit_core::{Mode, OutputInfo, Target};
use capit_ipc::{Event, Request, Response};

use eventline::{debug, error, info, warn};

use crate::{capture, overlay_region, overlay_screen, selection::SelectionState};

use super::notify;
use super::paths::default_output_path;
use super::state::DaemonState;

pub fn handle_request(
    state: &mut DaemonState,
    selection: &mut SelectionState,
    conn: &mut capit_ipc::ClientConn,
    req: Request,
) -> Response {
    // StartCapture FIRST
    if let Request::StartCapture { mode, target, with_ui } = req {
        info!(
            "StartCapture: mode={:?} target={:?} with_ui={}",
            mode, target, with_ui
        );

        return match mode {
            Mode::Region => {
                state.active_job = Some(Mode::Region);
                let _ = conn.send_event(Event::CaptureStarted { mode: Mode::Region });

                let target_output_idx = match determine_output_index(&state.outputs, target) {
                    Ok(idx) => idx,
                    Err(msg) => {
                        error!("determine_output_index failed: {}", msg);
                        state.active_job = None;
                        let _ = conn.send_event(Event::CaptureFailed { message: msg.clone() });
                        let _ = notify::notify_failed(&msg);
                        return Response::Error { message: msg };
                    }
                };

                handle_region_overlay_capture(state, conn, target_output_idx)
            }

            Mode::Screen => handle_screen_overlay_capture(state, conn, target),

            Mode::Window => {
                state.active_job = Some(Mode::Window);
                let _ = conn.send_event(Event::CaptureStarted { mode: Mode::Window });

                let msg = String::from(
                    "window capture is not implemented yet.\n\
                     planned backends: sway (ipc tree), hyprland (hyprctl), niri (ipc).",
                );

                warn!("{msg}");
                let _ = conn.send_event(Event::CaptureFailed { message: msg.clone() });
                let _ = notify::notify_failed(&msg);

                state.active_job = None;
                Response::Error { message: msg }
            }

            Mode::Record => {
                let msg = "record not implemented yet".to_string();
                let _ = conn.send_event(Event::CaptureFailed { message: msg.clone() });
                let _ = notify::notify_failed(&msg);
                Response::Error { message: msg }
            }
        };
    }

    // SetSelection / ConfirmSelection (selection-driven UI flow)
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
                                    let _ = conn.send_event(Event::CaptureFailed { message: msg.clone() });
                                    let _ = notify::notify_failed(&msg);
                                    state.active_job = None;
                                    return Response::Error { message: msg };
                                }
                            };

                            let out_path = default_output_path(&state.cfg, "png");
                            let result = capture::capture_screen_to_rect(&out_path, &rect);

                            match result {
                                Ok(()) => {
                                    let _ = conn.send_event(Event::CaptureFinished {
                                        path: out_path.display().to_string(),
                                    });
                                    let _ = notify::notify_saved(&out_path);
                                    state.active_job = None;
                                }
                                Err(msg) => {
                                    let _ = conn.send_event(Event::CaptureFailed { message: msg.clone() });
                                    let _ = notify::notify_failed(&msg);
                                    state.active_job = None;
                                    return Response::Error { message: msg };
                                }
                            }
                        }
                        other => {
                            let msg = format!("ConfirmSelection for {other:?} not implemented yet");
                            let _ = conn.send_event(Event::CaptureFailed { message: msg.clone() });
                            let _ = notify::notify_failed(&msg);
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

        Request::GetUiConfig => Response::UiConfig {
            cfg: state.ui.to_ipc(),
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
    target_output_idx: usize,
) -> Response {
    match overlay_region::run_region_overlay(state.outputs.clone(), target_output_idx) {
        Ok(Some(rect)) => {
            info!("overlay confirmed: {:?}", rect);

            let out_path = default_output_path(&state.cfg, "png");
            info!("capturing to: {}", out_path.display());

            match capture::capture_screen_to_rect(&out_path, &rect) {
                Ok(()) => {
                    info!("capture successful");
                    let _ = conn.send_event(Event::CaptureFinished {
                        path: out_path.display().to_string(),
                    });
                    let _ = notify::notify_saved(&out_path);
                    state.active_job = None;
                    Response::Ok
                }
                Err(msg) => {
                    error!("capture failed: {}", msg);
                    let _ = conn.send_event(Event::CaptureFailed { message: msg.clone() });
                    let _ = notify::notify_failed(&msg);
                    state.active_job = None;
                    Response::Error { message: msg }
                }
            }
        }
        Ok(None) => {
            // Cancel: do NOT notify (avoid spam)
            info!("overlay cancelled");
            let _ = conn.send_event(Event::CaptureFailed {
                message: "cancelled".into(),
            });
            state.active_job = None;
            Response::Ok
        }
        Err(msg) => {
            error!("overlay error: {}", msg);
            let _ = conn.send_event(Event::CaptureFailed { message: msg.clone() });
            let _ = notify::notify_failed(&msg);
            state.active_job = None;
            Response::Error { message: msg }
        }
    }
}

fn handle_screen_overlay_capture(
    state: &mut DaemonState,
    conn: &mut capit_ipc::ClientConn,
    target: Option<Target>,
) -> Response {
    state.active_job = Some(Mode::Screen);
    let _ = conn.send_event(Event::CaptureStarted { mode: Mode::Screen });

    let initial_idx = match &target {
        Some(Target::OutputName(name)) => state
            .outputs
            .iter()
            .position(|o| o.name.as_deref() == Some(name.as_str())),
        _ => None,
    };

    let picked = match overlay_screen::run_screen_overlay(state.outputs.clone(), initial_idx) {
        Ok(Some(t)) => t,
        Ok(None) => {
            // Cancel: do NOT notify
            info!("screen overlay cancelled");
            let _ = conn.send_event(Event::CaptureFailed {
                message: "cancelled".into(),
            });
            state.active_job = None;
            return Response::Ok;
        }
        Err(msg) => {
            error!("screen overlay error: {}", msg);
            let _ = conn.send_event(Event::CaptureFailed { message: msg.clone() });
            let _ = notify::notify_failed(&msg);
            state.active_job = None;
            return Response::Error { message: msg };
        }
    };

    let out_path = default_output_path(&state.cfg, "png");
    info!("capturing to: {}", out_path.display());

    let result: std::result::Result<(), String> = match picked {
        Target::AllScreens => capture::capture_screen_to(&out_path),

        Target::OutputName(name) => match state
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

        other => Err(format!("overlay returned unsupported target: {other:?}")),
    };

    match result {
        Ok(()) => {
            let _ = conn.send_event(Event::CaptureFinished {
                path: out_path.display().to_string(),
            });
            let _ = notify::notify_saved(&out_path);
            state.active_job = None;
            Response::Ok
        }
        Err(msg) => {
            error!("capture failed: {}", msg);
            let _ = conn.send_event(Event::CaptureFailed { message: msg.clone() });
            let _ = notify::notify_failed(&msg);
            state.active_job = None;
            Response::Error { message: msg }
        }
    }
}

fn determine_output_index(
    outputs: &[OutputInfo],
    target: Option<Target>,
) -> std::result::Result<usize, String> {
    if outputs.is_empty() {
        return Err("no outputs available".into());
    }

    match target {
        None | Some(Target::AllScreens) => Ok(0),
        Some(Target::OutputName(name)) => outputs
            .iter()
            .position(|o| o.name.as_deref() == Some(name.as_str()))
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
