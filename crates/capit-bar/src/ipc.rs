// Author: Dustin Pilgrim
// License: MIT

use std::path::Path;
use std::fmt;

use capit_ipc::IpcClient;
use eventline::error;

#[derive(Debug)]
pub struct ConnectError(String);

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConnectError {}

pub fn connect(socket: &Path) -> Result<IpcClient, ConnectError> {
    IpcClient::connect(socket).map_err(|e| {
        error!("failed to connect to daemon: {e}");

        ConnectError(format!(
            "capit-bar: cannot connect to capitd at {}\n\
             → {}\n\
             Hint: start the daemon with `capitd`.",
            socket.display(),
            e
        ))
    })
}
