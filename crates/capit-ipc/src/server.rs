// Author: Dustin Pilgrim
// License: MIT

use std::fs;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use crate::error::{IpcError, Result};
use crate::framing::{read_frame, write_frame};
use crate::protocol::{Event, Request, Response, Wire, IPC_VERSION};

pub struct IpcServer {
    listener: UnixListener,
    socket_path: PathBuf,
    max_frame: usize,
}

pub struct ClientConn {
    stream: UnixStream,
    max_frame: usize,
}

impl IpcServer {
    pub fn bind(socket_path: impl AsRef<Path>) -> Result<Self> {
        let socket_path = socket_path.as_ref().to_path_buf();

        // remove stale socket
        let _ = fs::remove_file(&socket_path);

        let listener = UnixListener::bind(&socket_path)?;
        Ok(Self {
            listener,
            socket_path,
            max_frame: 1024 * 1024,
        })
    }

    pub fn accept(&self) -> Result<ClientConn> {
        let (stream, _addr) = self.listener.accept()?;
        Ok(ClientConn {
            stream,
            max_frame: self.max_frame,
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> Result<()> {
        self.listener.set_nonblocking(nonblocking)?;
        Ok(())
    }
}

impl ClientConn {
    pub fn recv(&mut self) -> Result<Request> {
        let bytes = read_frame(&mut self.stream, self.max_frame)?;
        let req: Request = postcard::from_bytes(&bytes)?;
        Ok(req)
    }

    pub fn send(&mut self, resp: Response) -> Result<()> {
        let bytes = postcard::to_allocvec(&Wire::Response(resp))?;
        write_frame(&mut self.stream, &bytes)?;
        Ok(())
    }

    pub fn send_event(&mut self, ev: Event) -> Result<()> {
        let bytes = postcard::to_allocvec(&Wire::Event(ev))?;
        write_frame(&mut self.stream, &bytes)?;
        Ok(())
    }

    pub fn handle_hello(&mut self, req: &Request) -> Result<()> {
        match req {
            Request::Hello(h) if h.version == IPC_VERSION => self.send(Response::Ok),
            Request::Hello(h) => Err(IpcError::VersionMismatch {
                client: h.version,
                server: IPC_VERSION,
            }),
            _ => self.send(Response::Error {
                message: "expected hello".into(),
            }),
        }
    }
}
