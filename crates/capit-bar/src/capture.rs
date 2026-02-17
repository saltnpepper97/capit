// Author: Dustin Pilgrim
// License: MIT

use capit_core::{Mode, Target};
use capit_ipc::{Event, IpcClient, Request, Response};
use eventline::{debug, error, info};

use crate::print;

#[derive(Debug)]
pub enum CaptureOutcome {
    Finished { path: String },
    Cancelled,
}

pub fn start_capture(
    client: &mut IpcClient,
    mode: Mode,
    target: Option<Target>,
    with_ui: bool,
) -> Result<CaptureOutcome, String> {
    debug!(
        "start_capture: mode={:?}, target={:?}, with_ui={}",
        mode, target, with_ui
    );

    let resp = client
        .call(Request::StartCapture { mode, target, with_ui })
        .map_err(|e| format!("{e}"))?;

    match resp {
        Response::Ok => debug!("StartCapture accepted, waiting for events"),
        other => {
            print::print_response(other);
            return Ok(CaptureOutcome::Cancelled);
        }
    }

    loop {
        let ev = client.next_event().map_err(|e| format!("{e}"))?;
        debug!("event: {:?}", ev);

        match ev {
            Event::CaptureFinished { path } => {
                info!("capture finished: {}", path);
                return Ok(CaptureOutcome::Finished { path });
            }
            Event::CaptureFailed { message } => {
                if message == "cancelled" {
                    info!("capture cancelled");
                    return Ok(CaptureOutcome::Cancelled);
                }
                error!("capture failed: {}", message);
                return Err(message);
            }
            _ => {}
        }
    }
}
