// Author: Dustin Pilgrim
// License: MIT

use capit_core::Mode;

use smithay_client_toolkit::{
    output::OutputState,
    registry::RegistryState,
};

use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_compositor, wl_seat, wl_shm},
    Connection,
};

use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1;

use super::app::App;

// Defaults/fallbacks (daemon should override via IPC)
const DEFAULT_ACCENT: u32 = 0xFF0A_84FF;
const DEFAULT_BAR_BG: u32 = 0xFF0F_1115;

pub fn run_bar(accent_colour: u32, bar_background_colour: u32) -> Result<Option<Mode>, String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("wayland connect: {e}"))?;

    let (globals, mut queue) =
        registry_queue_init(&conn).map_err(|e| format!("registry init: {e}"))?;
    let qh = queue.handle();

    let registry_state = RegistryState::new(&globals);
    let output_state = OutputState::new(&globals, &qh);

    let accent = if accent_colour == 0 { DEFAULT_ACCENT } else { accent_colour };
    let bg = if bar_background_colour == 0 { DEFAULT_BAR_BG } else { bar_background_colour };

    let mut app = App::new(registry_state, output_state, accent, bg);

    app.compositor = globals
        .bind::<wl_compositor::WlCompositor, _, _>(&qh, 1..=6, ())
        .ok();
    app.shm = globals.bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ()).ok();
    app.seat = globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=7, ()).ok();
    app.layer_shell = globals
        .bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(&qh, 1..=4, ())
        .ok();

    queue.roundtrip(&mut app).map_err(|e| format!("roundtrip: {e}"))?;

    if app.compositor.is_none() {
        return Err("wl_compositor not available".into());
    }
    if app.layer_shell.is_none() {
        return Err("zwlr_layer_shell_v1 not available".into());
    }
    if app.shm.is_none() {
        return Err("wl_shm not available".into());
    }
    if app.seat.is_none() {
        return Err("wl_seat not available".into());
    }

    app.ensure_surface(&qh)?;
    app.init_cursor(&conn, &qh)?;

    queue.roundtrip(&mut app).map_err(|e| format!("roundtrip2: {e}"))?;

    while !app.is_finished() {
        queue
            .blocking_dispatch(&mut app)
            .map_err(|e| format!("dispatch: {e}"))?;
        let _ = conn.flush();
    }

    Ok(app.result.unwrap_or(None))
}
