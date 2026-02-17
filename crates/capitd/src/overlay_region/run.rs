// Author: Dustin Pilgrim
// License: MIT

use capit_core::{OutputInfo, Rect};

use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_compositor, wl_seat, wl_shm};
use wayland_client::Connection;

use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::registry::RegistryState;

use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1;

use super::app::App;

pub fn run_region_overlay(
    all_outputs: Vec<OutputInfo>,
    target_output_idx: usize,
    accent_colour: u32,
) -> Result<Option<Rect>, String> {
    if all_outputs.is_empty() {
        return Err("no outputs available".into());
    }
    if target_output_idx >= all_outputs.len() {
        return Err(format!(
            "target output index {} out of bounds",
            target_output_idx
        ));
    }

    let conn = Connection::connect_to_env().map_err(|e| format!("wayland connect: {e}"))?;
    let (globals, mut queue) =
        registry_queue_init(&conn).map_err(|e| format!("registry init: {e}"))?;
    let qh = queue.handle();

    let registry_state = RegistryState::new(&globals);
    let output_state = OutputState::new(&globals, &qh);

    let mut app = App::new(
        registry_state,
        output_state,
        all_outputs,
        target_output_idx,
        accent_colour,
    );

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

    // Cursor setup (must be after shm/compositor exist).
    app.init_cursor(&conn, &qh)?;

    super::surfaces::try_create_surfaces(&mut app, &qh)?;

    if !app.surfaces_created {
        queue.roundtrip(&mut app).map_err(|e| format!("roundtrip2: {e}"))?;
        super::surfaces::try_create_surfaces(&mut app, &qh)?;
    }

    if !app.surfaces_created {
        return Err("Failed to create surfaces".into());
    }

    while !app.is_finished() {
        queue.blocking_dispatch(&mut app).map_err(|e| format!("dispatch: {e}"))?;
        let _ = conn.flush();
    }

    Ok(app.result.unwrap_or(None))
}
