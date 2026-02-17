// Author: Dustin Pilgrim
// License: MIT

use capit_core::OutputInfo;

use smithay_client_toolkit::output::OutputState;

use wayland_client::protocol::{wl_output, wl_surface, wl_shm, wl_buffer};
use wayland_client::{QueueHandle, Proxy};

use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1,
    zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity},
};

use super::app::App;
use super::shm::ShmBuffer;

pub struct OutputSurface {
    pub output_info: OutputInfo,
    pub wl_output: wl_output::WlOutput,
    pub surface: wl_surface::WlSurface,
    pub layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    pub shm_buf: Option<ShmBuffer>,
    pub configured: bool,
}

pub fn try_create_surfaces(app: &mut App, qh: &QueueHandle<App>) -> Result<(), String> {
    if app.surfaces_created {
        return Ok(());
    }

    let compositor = app.compositor.as_ref().ok_or("no compositor")?;
    let layer_shell = app.layer_shell.as_ref().ok_or("no layer_shell")?;
    let shm = app.shm.as_ref().ok_or("no shm")?;

    for output_info in &app.outputs {
        let wl_output = app.output_state.outputs().into_iter().find(|wl_out| {
            app.output_state
                .info(wl_out)
                .map(|info| info.name.as_ref() == output_info.name.as_ref())
                .unwrap_or(false)
        });

        let wl_output = match wl_output {
            Some(o) => o,
            None => {
                eprintln!("Warning: could not match wl_output for {:?}", output_info.name);
                continue;
            }
        };

        let width = output_info.width.max(1);
        let height = output_info.height.max(1);

        let surface = compositor.create_surface(qh, ());
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            Some(&wl_output),
            zwlr_layer_shell_v1::Layer::Overlay,
            "capit-screen".into(),
            qh,
            (),
        );

        layer_surface.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);

        if app.output_surfaces.is_empty() {
            layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        } else {
            layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        }

        layer_surface.set_exclusive_zone(-1);
        layer_surface.set_size(0, 0);

        let shm_buf = ShmBuffer::new(shm, qh, width, height)?;

        app.output_surfaces.push(OutputSurface {
            output_info: output_info.clone(),
            wl_output: wl_output.clone(),
            surface: surface.clone(),
            layer_surface,
            shm_buf: Some(shm_buf),
            configured: false,
        });

        surface.commit();
    }

    app.surfaces_created = true;

    // Ensure keyboard-only users have a selection immediately.
    if app.hovered_output_idx.is_none() && !app.outputs.is_empty() {
        app.hovered_output_idx = Some(0);
    }

    Ok(())
}

pub fn handle_layer_configure(
    app: &mut App,
    proxy: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    width: u32,
    height: u32,
    qh: &QueueHandle<App>,
) {
    if let Some(os) = app.output_surfaces.iter_mut().find(|os| &os.layer_surface == proxy) {
        let needs_resize = os
            .shm_buf
            .as_ref()
            .map_or(true, |b| b.width != width as i32 || b.height != height as i32);

        if needs_resize && width > 0 && height > 0 {
            if let Some(shm) = app.shm.as_ref() {
                if let Ok(new_buf) = ShmBuffer::new(shm, qh, width as i32, height as i32) {
                    os.shm_buf = Some(new_buf);
                }
            }
        }
        os.configured = true;
    }
}

pub fn handle_buffer_release(app: &mut App, buffer: &wl_buffer::WlBuffer) {
    for os in &mut app.output_surfaces {
        if let Some(ref mut sb) = os.shm_buf {
            if &sb.buffer == buffer {
                sb.busy = false;
                break;
            }
        }
    }
}
