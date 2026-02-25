// Author: Dustin Pilgrim
// License: MIT

use std::path::Path;

use capit_ipc::IpcClient;
use eventline::error;

pub fn connect(socket: &Path) -> Result<IpcClient, String> {
    IpcClient::connect(socket).map_err(|e| {
        // keep structured log for debugging
        error!("failed to connect to daemon: {e}");

        // user-facing message (printed by main.rs)
        format!(
            "capit: cannot connect to capitd at {}\n\
             → {}\n\
             Hint: start the daemon with `capitd`.",
            socket.display(),
            e
        )
    })
}
