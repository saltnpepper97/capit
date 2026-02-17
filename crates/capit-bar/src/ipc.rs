// Author: Dustin Pilgrim
// License: MIT

use std::path::Path;

use capit_ipc::IpcClient;
use eventline::error;

pub fn connect(socket: &Path) -> Result<IpcClient, String> {
    IpcClient::connect(socket).map_err(|e| {
        error!("failed to connect to daemon: {e}");
        format!(
            "Failed to connect to daemon at {}: {e}\nIs capitd running?",
            socket.display()
        )
    })
}
