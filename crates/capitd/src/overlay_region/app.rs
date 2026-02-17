// Author: Dustin Pilgrim
// License: MIT
//
// Region overlay using SCTK for proper output handling

use capit_core::{OutputInfo, Rect};

use smithay_client_toolkit::{
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
};

use wayland_client::{
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_shm_pool,
        wl_surface,
    },
    Connection, Dispatch, QueueHandle, WEnum, Proxy,
};

use wayland_cursor::CursorTheme;

use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use super::model::{self, DragMode, RectLocal};
use super::surfaces::OutputSurface;

const BTN_LEFT: u32 = 272;
const KEY_ESC: u32 = 1;
const KEY_ENTER: u32 = 28;

// Same default you use elsewhere (bar, etc.)
const DEFAULT_ACCENT: u32 = 0xFF0A_84FF;

pub struct App {
    // SCTK state
    pub registry_state: RegistryState,
    pub output_state: OutputState,

    // Our data
    pub outputs: Vec<OutputInfo>,
    pub target_output_idx: usize,

    pub desktop_min_x: i32,
    pub desktop_min_y: i32,
    pub desktop_max_x: i32,
    pub desktop_max_y: i32,

    // Theme
    pub accent_colour: u32,

    // Wayland objects
    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub seat: Option<wl_seat::WlSeat>,
    pub layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,

    // Surfaces - created after matching outputs by name
    pub output_surfaces: Vec<OutputSurface>,
    pub surfaces_created: bool,

    pub pointer: Option<wl_pointer::WlPointer>,
    pub keyboard: Option<wl_keyboard::WlKeyboard>,
    pub current_output_idx: Option<usize>,

    // Cursor support
    pub cursor_surface: Option<wl_surface::WlSurface>,
    pub cursor_theme: Option<CursorTheme>,
    pub cursor_name: &'static str, // e.g. "crosshair"

    pub cursor: (i32, i32),
    pub selection: RectLocal,

    pub drag_mode: DragMode,
    pub grab_cursor: (i32, i32),
    pub grab_rect: RectLocal,

    pub pending_redraw: bool,
    pub result: Option<Option<Rect>>,
}

impl App {
    pub fn new(
        registry_state: RegistryState,
        output_state: OutputState,
        outputs: Vec<OutputInfo>,
        target_output_idx: usize,
        accent_colour: u32,
    ) -> Self {
        let (min_x, min_y, max_x, max_y) = outputs.iter().fold(
            (i32::MAX, i32::MAX, i32::MIN, i32::MIN),
            |(min_x, min_y, max_x, max_y), o| {
                (
                    min_x.min(o.x),
                    min_y.min(o.y),
                    max_x.max(o.x + o.width),
                    max_y.max(o.y + o.height),
                )
            },
        );

        let target_output = &outputs[target_output_idx];
        let init_w = (target_output.width / 2).clamp(260, target_output.width.max(1));
        let init_h = (target_output.height / 2).clamp(180, target_output.height.max(1));
        let init_x = target_output.x + (target_output.width - init_w) / 2;
        let init_y = target_output.y + (target_output.height - init_h) / 2;

        let accent = if accent_colour == 0 { DEFAULT_ACCENT } else { accent_colour };

        Self {
            registry_state,
            output_state,
            outputs,
            target_output_idx,
            desktop_min_x: min_x,
            desktop_min_y: min_y,
            desktop_max_x: max_x,
            desktop_max_y: max_y,

            accent_colour: accent,

            compositor: None,
            shm: None,
            seat: None,
            layer_shell: None,

            output_surfaces: Vec::new(),
            surfaces_created: false,

            pointer: None,
            keyboard: None,
            current_output_idx: None,

            cursor_surface: None,
            cursor_theme: None,
            cursor_name: "crosshair",

            cursor: (init_x + init_w / 2, init_y + init_h / 2),
            selection: RectLocal {
                x: init_x,
                y: init_y,
                w: init_w.max(model::MIN_W),
                h: init_h.max(model::MIN_H),
            },

            drag_mode: DragMode::None,
            grab_cursor: (0, 0),
            grab_rect: RectLocal::default(),

            pending_redraw: true,
            result: None,
        }
    }

    pub fn init_cursor(&mut self, conn: &Connection, qh: &QueueHandle<Self>) -> Result<(), String> {
        if self.cursor_theme.is_some() {
            return Ok(());
        }
        let compositor = self.compositor.as_ref().ok_or("no compositor")?;
        let shm = self.shm.as_ref().ok_or("no shm")?;

        let theme = CursorTheme::load(conn, shm.clone(), 32)
            .map_err(|e| format!("cursor: load theme: {e:?}"))?;
        let surf = compositor.create_surface(qh, ());

        self.cursor_theme = Some(theme);
        self.cursor_surface = Some(surf);
        Ok(())
    }

    pub fn set_cursor_image(&mut self, pointer: &wl_pointer::WlPointer, serial: u32) {
        let (Some(theme), Some(surf)) = (self.cursor_theme.as_mut(), self.cursor_surface.as_ref())
        else {
            return;
        };

        let cursor = {
            let c = theme.get_cursor(self.cursor_name);
            if c.is_some() {
                c
            } else {
                theme.get_cursor("left_ptr")
            }
        };

        let Some(cursor) = cursor else { return; };

        let img = &cursor[0];
        let (hx, hy) = img.hotspot();
        pointer.set_cursor(serial, Some(surf), hx as i32, hy as i32);

        surf.attach(Some(&**img), 0, 0);
        surf.commit();
    }

    pub fn cancel(&mut self) {
        self.result = Some(None);
    }

    pub fn confirm(&mut self) {
        let mut r = self.selection;
        r.clamp_to(self.desktop_min_x, self.desktop_min_y, self.desktop_max_x, self.desktop_max_y);
        self.result = Some(Some(Rect {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
        }));
    }

    pub fn is_finished(&self) -> bool {
        self.result.is_some()
    }

    pub fn request_redraw(&mut self) {
        let any_busy = self
            .output_surfaces
            .iter()
            .any(|os| os.shm_buf.as_ref().map_or(false, |b| b.busy));

        if any_busy {
            self.pending_redraw = true;
            return;
        }

        let _ = super::render::redraw_all(self);
    }
}

// SCTK trait implementations
impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {
        let _ = super::surfaces::try_create_surfaces(self, qh);
    }

    fn update_output(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {
        let _ = super::surfaces::try_create_surfaces(self, qh);
    }

    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
}

// Dispatch implementations
impl Dispatch<wl_compositor::WlCompositor, ()> for App {
    fn event(_: &mut Self, _: &wl_compositor::WlCompositor, _: wl_compositor::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_shm::WlShm, ()> for App {
    fn event(_: &mut Self, _: &wl_shm::WlShm, _: wl_shm::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for App {
    fn event(_: &mut Self, _: &wl_shm_pool::WlShmPool, _: wl_shm_pool::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_surface::WlSurface, ()> for App {
    fn event(_: &mut Self, _: &wl_surface::WlSurface, _: wl_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for App {
    fn event(_: &mut Self, _: &zwlr_layer_shell_v1::ZwlrLayerShellV1, _: zwlr_layer_shell_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for App {
    fn event(
        state: &mut Self,
        proxy: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                proxy.ack_configure(serial);

                super::surfaces::handle_layer_configure(state, proxy, width, height, qh);

                state.selection.clamp_to(
                    state.desktop_min_x,
                    state.desktop_min_y,
                    state.desktop_max_x,
                    state.desktop_max_y,
                );

                state.pending_redraw = true;
                state.request_redraw();
            }
            zwlr_layer_surface_v1::Event::Closed => state.cancel(),
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for App {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            if let WEnum::Value(caps) = capabilities {
                if caps.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                    state.pointer = Some(seat.get_pointer(qh, ()));

                    if let Err(e) = state.init_cursor(conn, qh) {
                        eprintln!("Failed to init cursor: {}", e);
                    }
                }
                if caps.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
                    state.keyboard = Some(seat.get_keyboard(qh, ()));
                }
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for App {
    fn event(
        state: &mut Self,
        pointer: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { serial, surface_x, surface_y, surface, .. } => {
                state.set_cursor_image(pointer, serial);

                if let Some((idx, os)) = state
                    .output_surfaces
                    .iter()
                    .enumerate()
                    .find(|(_, os)| os.surface.id() == surface.id())
                {
                    state.current_output_idx = Some(idx);
                    let global_x = surface_x as i32 + os.output_info.x;
                    let global_y = surface_y as i32 + os.output_info.y;
                    state.cursor = (global_x, global_y);
                    state.request_redraw();
                }
            }

            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                if let Some(idx) = state.current_output_idx {
                    if let Some(os) = state.output_surfaces.get(idx) {
                        let global_x = surface_x as i32 + os.output_info.x;
                        let global_y = surface_y as i32 + os.output_info.y;
                        state.cursor = (global_x, global_y);

                        if !matches!(state.drag_mode, DragMode::None) {
                            state.selection = model::apply_drag(
                                state.drag_mode,
                                state.cursor,
                                state.grab_cursor,
                                state.grab_rect,
                                state.desktop_min_x,
                                state.desktop_min_y,
                                state.desktop_max_x,
                                state.desktop_max_y,
                            );
                        }

                        state.request_redraw();
                    }
                }
            }

            wl_pointer::Event::Button { button, state: btn_state, .. } => {
                if button != BTN_LEFT {
                    return;
                }

                match btn_state {
                    WEnum::Value(wl_pointer::ButtonState::Pressed) => {
                        state.grab_cursor = state.cursor;
                        state.grab_rect = state.selection;
                        state.drag_mode = model::hit_test(state.selection, state.cursor.0, state.cursor.1);

                        // preserve your original special-case behavior
                        if matches!(state.drag_mode, DragMode::Resize(_))
                            && !state.selection.contains(state.cursor.0, state.cursor.1)
                        {
                            state.grab_cursor = (
                                state.grab_rect.x + state.grab_rect.w,
                                state.grab_rect.y + state.grab_rect.h,
                            );
                        }

                        state.request_redraw();
                    }

                    WEnum::Value(wl_pointer::ButtonState::Released) => {
                        state.drag_mode = DragMode::None;
                        state.request_redraw();
                    }

                    _ => {}
                }
            }

            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for App {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Key { key, state: key_state, .. } => {
                if key_state != WEnum::Value(wl_keyboard::KeyState::Pressed) {
                    return;
                }
                if key == KEY_ESC {
                    state.cancel();
                } else if key == KEY_ENTER {
                    state.confirm();
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for App {
    fn event(
        state: &mut Self,
        buffer: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            super::surfaces::handle_buffer_release(state, buffer);
            if state.pending_redraw {
                state.request_redraw();
            }
        }
    }
}

// SCTK delegates
smithay_client_toolkit::delegate_output!(App);
smithay_client_toolkit::delegate_registry!(App);
