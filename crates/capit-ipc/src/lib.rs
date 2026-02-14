// Author: Dustin Pilgrim
// License: MIT

pub mod protocol;
pub mod framing;
pub mod client;
pub mod server;
pub mod error;

pub use protocol::{Request, Response, Event, IpcHello, IPC_VERSION};
pub use client::IpcClient;
pub use server::{IpcServer, ClientConn};
pub use error::{IpcError, Result};
