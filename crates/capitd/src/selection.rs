// Author: Dustin Pilgrim
// License: MIT

use capit_core::{Mode, Rect, Target};
use capit_ipc::{Event, Request, Response};

#[derive(Debug, Clone)]
pub struct ActiveSelection {
    pub mode: Mode,
    pub target: Option<Target>,
    pub rect: Option<Rect>,
}

#[derive(Debug, Default)]
pub struct SelectionState {
    active: Option<ActiveSelection>,
}

impl SelectionState {
    pub fn new() -> Self {
        Self { active: None }
    }

    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub fn active_mode(&self) -> Option<Mode> {
        self.active.as_ref().map(|s| s.mode)
    }

    /// Handle a request that is related to interactive selection mode.
    ///
    /// - `emit` is a callback that can push async events back to the client.
    /// - Returns `Some(Response)` if the request was handled here.
    /// - Returns `None` if it's not a selection-related request.
    pub fn handle_request<F>(&mut self, req: &Request, mut emit: F) -> Option<Response>
    where
        F: FnMut(Event),
    {
        match req {
            Request::StartCapture { mode, target, with_ui } => {
                if *with_ui && *mode == Mode::Region {
                    self.active = Some(ActiveSelection {
                        mode: *mode,
                        target: target.clone(),
                        rect: None,
                    });

                    emit(Event::CaptureStarted { mode: *mode });
                    return Some(Response::Ok);
                }

                None
            }

            Request::SetSelection { rect } => {
                if let Some(sel) = self.active.as_mut() {
                    sel.rect = Some(rect.clone());

                    // Echo back (later: clamp/snap here)
                    emit(Event::SelectionPreview { rect: rect.clone() });
                    return Some(Response::Ok);
                }

                Some(Response::Error {
                    message: "no active selection session".into(),
                })
            }

            Request::ConfirmSelection => {
                if let Some(sel) = self.active.as_ref() {
                    if sel.rect.is_none() {
                        return Some(Response::Error {
                            message: "no selection rect set".into(),
                        });
                    }

                    // NOTE: actual capture happens in daemon main handler,
                    // because it needs access to capture backend / filesystem.
                    // We just say "ok, confirm accepted".
                    return Some(Response::Ok);
                }

                Some(Response::Error {
                    message: "no active selection session".into(),
                })
            }

            Request::Cancel => {
                if self.active.is_some() {
                    self.active = None;
                    return Some(Response::Ok);
                }
                None
            }

            _ => None,
        }
    }

    /// Take (consume) the active selection when you’re ready to execute capture.
    /// Used after ConfirmSelection is received and validated.
    pub fn take_active(&mut self) -> Option<ActiveSelection> {
        self.active.take()
    }

    /// Peek current selection (for debug / status)
    pub fn peek_active(&self) -> Option<&ActiveSelection> {
        self.active.as_ref()
    }
}
