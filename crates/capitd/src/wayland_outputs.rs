// Author: Dustin Pilgrim
// License: MIT
// Using SCTK for proper xdg-output support

use capit_core::OutputInfo;
use smithay_client_toolkit::{
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::wl_output,
    Connection, QueueHandle,
};

struct AppData {
    registry_state: RegistryState,
    output_state: OutputState,
}

impl ProvidesRegistryState for AppData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState];
}

impl OutputHandler for AppData {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

pub fn query_outputs() -> Result<Vec<OutputInfo>, String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("wayland connect: {e}"))?;

    let (globals, mut event_queue) = registry_queue_init(&conn)
        .map_err(|e| format!("registry init: {e}"))?;

    let qh = event_queue.handle();
    let registry_state = RegistryState::new(&globals);
    let output_state = OutputState::new(&globals, &qh);

    let mut app_data = AppData {
        registry_state,
        output_state,
    };

    // Process initial events
    event_queue
        .roundtrip(&mut app_data)
        .map_err(|e| format!("roundtrip 1: {e}"))?;

    // Give time for xdg-output events
    event_queue
        .roundtrip(&mut app_data)
        .map_err(|e| format!("roundtrip 2: {e}"))?;

    // Collect output info
    let mut infos: Vec<OutputInfo> = Vec::new();

    for output in app_data.output_state.outputs() {
        let info_opt = app_data.output_state.info(&output);
        
        if let Some(info) = info_opt {
            // SCTK provides logical geometry via xdg-output when available
            let logical_pos = info.logical_position;
            let logical_size = info.logical_size;
            
            let output_info = OutputInfo {
                name: info.name.clone(),
                x: logical_pos.map(|(x, _)| x).unwrap_or(0),
                y: logical_pos.map(|(_, y)| y).unwrap_or(0),
                width: logical_size.map(|(w, _)| w as i32).unwrap_or(0),
                height: logical_size.map(|(_, h)| h as i32).unwrap_or(0),
                scale: info.scale_factor,
            };
            
            infos.push(output_info);
        }
    }

    // Sort by position for consistent ordering
    infos.sort_by_key(|info| (info.y, info.x));

    Ok(infos)
}

// Required trait implementations
smithay_client_toolkit::delegate_output!(AppData);
smithay_client_toolkit::delegate_registry!(AppData);
