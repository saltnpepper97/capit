// Author: Dustin Pilgrim
// License: MIT

use capit_core::OutputInfo;

use wayland_client::protocol::{wl_output, wl_surface};
use wayland_client::QueueHandle;

use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1,
    zwlr_layer_surface_v1,
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
        // Match wl_outputs to OutputInfo by name
        let wl_output = app
            .output_state
            .outputs()
            .into_iter()
            .find(|wl_out| {
                if let Some(info) = app.output_state.info(wl_out) {
                    info.name.as_ref() == output_info.name.as_ref()
                } else {
                    false
                }
            });

        let wl_output = match wl_output {
            Some(o) => o,
            None => {
                eprintln!("Warning: Could not match wl_output for {:?}", output_info.name);
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
            "capit-region".into(),
            qh,
            (),
        );

        layer_surface.set_anchor(
            zwlr_layer_surface_v1::Anchor::Top
                | zwlr_layer_surface_v1::Anchor::Bottom
                | zwlr_layer_surface_v1::Anchor::Left
                | zwlr_layer_surface_v1::Anchor::Right,
        );

        if app.output_surfaces.is_empty() {
            layer_surface.set_keyboard_interactivity(
                zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive,
            );
        } else {
            layer_surface.set_keyboard_interactivity(
                zwlr_layer_surface_v1::KeyboardInteractivity::None,
            );
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
    Ok(())
}

pub fn handle_layer_configure(
    app: &mut App,
    proxy: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    width: u32,
    height: u32,
    qh: &QueueHandle<App>,
) {
    if let Some(output_surface) = app
        .output_surfaces
        .iter_mut()
        .find(|os| &os.layer_surface == proxy)
    {
        let needs_resize = output_surface
            .shm_buf
            .as_ref()
            .map_or(true, |b| b.width != width as i32 || b.height != height as i32);

        if needs_resize && width > 0 && height > 0 {
            if let Some(shm) = app.shm.as_ref() {
                if let Ok(new_buf) = ShmBuffer::new(shm, qh, width as i32, height as i32) {
                    output_surface.shm_buf = Some(new_buf);
                }
            }
        }

        output_surface.configured = true;
    }
}

pub fn handle_buffer_release(app: &mut App, buffer: &wayland_client::protocol::wl_buffer::WlBuffer) {
    for os in &mut app.output_surfaces {
        if let Some(ref mut sb) = os.shm_buf {
            if &sb.buffer == buffer {
                sb.busy = false;
                break;
            }
        }
    }
}
