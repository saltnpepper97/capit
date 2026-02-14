// Author: Dustin Pilgrim
// License: MIT

use std::collections::VecDeque;
use std::os::unix::net::UnixStream;
use std::path::Path;

use crate::error::{IpcError, Result};
use crate::framing::{read_frame, write_frame};
use crate::protocol::{Event, IpcHello, Request, Response, Wire, IPC_VERSION};

pub struct IpcClient {
    stream: UnixStream,
    max_frame: usize,
    pending_events: VecDeque<Event>,
}

impl IpcClient {
    pub fn connect(socket_path: impl AsRef<Path>) -> Result<Self> {
        let stream = UnixStream::connect(socket_path)?;
        let mut this = Self {
            stream,
            max_frame: 1024 * 1024,
            pending_events: VecDeque::new(),
        };

        let resp = this.call(Request::Hello(IpcHello { version: IPC_VERSION }))?;
        match resp {
            Response::Ok => Ok(this),
            Response::Error { message } => Err(IpcError::Remote(message)),
            _ => Err(IpcError::Remote("unexpected hello response".into())),
        }
    }

    pub fn call(&mut self, req: Request) -> Result<Response> {
        let bytes = postcard::to_allocvec(&req)?;
        write_frame(&mut self.stream, &bytes)?;

        loop {
            match self.recv_wire()? {
                Wire::Response(resp) => return Ok(resp),
                Wire::Event(ev) => {
                    self.pending_events.push_back(ev);
                    continue;
                }
            }
        }
    }

    pub fn next_event(&mut self) -> Result<Event> {
        if let Some(ev) = self.pending_events.pop_front() {
            return Ok(ev);
        }

        loop {
            match self.recv_wire()? {
                Wire::Event(ev) => return Ok(ev),
                Wire::Response(_) => {
                    continue;
                }
            }
        }
    }

    fn recv_wire(&mut self) -> Result<Wire> {
        let bytes = read_frame(&mut self.stream, self.max_frame)?;
        let msg: Wire = postcard::from_bytes(&bytes)?;
        Ok(msg)
    }
}
