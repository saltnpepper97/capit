// Author: Dustin Pilgrim
// License: MIT

use capit_core::{OutputInfo, Target};

use smithay_client_toolkit::{
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
};

use wayland_client::{
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_shm_pool, wl_surface,
    },
    Connection, Dispatch, QueueHandle, WEnum, Proxy,
};

use wayland_cursor::CursorTheme;

use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1,
    zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity},
};

use super::surfaces::OutputSurface;

const BTN_LEFT: u32 = 272;
const KEY_ESC: u32 = 1;
const KEY_ENTER: u32 = 28;

pub struct App {
    pub registry_state: RegistryState,
    pub output_state: OutputState,

    pub outputs: Vec<OutputInfo>,
    pub hovered_output_idx: Option<usize>,

    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub seat: Option<wl_seat::WlSeat>,
    pub layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,

    pub output_surfaces: Vec<OutputSurface>,
    pub surfaces_created: bool,

    pub pointer: Option<wl_pointer::WlPointer>,
    pub keyboard: Option<wl_keyboard::WlKeyboard>,
    pub current_surface_idx: Option<usize>,

    pub cursor_surface: Option<wl_surface::WlSurface>,
    pub cursor_theme: Option<CursorTheme>,
    pub cursor_name: &'static str,

    pub pending_redraw: bool,
    pub result: Option<Option<Target>>,
}

impl App {
    pub fn new(
        registry_state: RegistryState,
        output_state: OutputState,
        outputs: Vec<OutputInfo>,
        initial_output_idx: Option<usize>,
    ) -> Self {
        Self {
            registry_state,
            output_state,
            outputs,
            hovered_output_idx: initial_output_idx,
            compositor: None,
            shm: None,
            seat: None,
            layer_shell: None,
            output_surfaces: Vec::new(),
            surfaces_created: false,
            pointer: None,
            keyboard: None,
            current_surface_idx: None,
            cursor_surface: None,
            cursor_theme: None,
            cursor_name: "left_ptr",
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

        let cursor = match theme.get_cursor(self.cursor_name) {
            Some(c) => Some(c),
            None => theme.get_cursor("left_ptr"),
        };

        let Some(cursor) = cursor else { return; };

        let img = &cursor[0];
        let (hx, hy) = img.hotspot();
        pointer.set_cursor(serial, Some(surf), hx as i32, hy as i32);

        surf.attach(Some(&**img), 0, 0);
        surf.commit();
    }

    pub fn is_finished(&self) -> bool { self.result.is_some() }
    pub fn cancel(&mut self) { self.result = Some(None); }
    pub fn confirm_all(&mut self) { self.result = Some(Some(Target::AllScreens)); }

    pub fn confirm_hovered(&mut self) {
        let idx = match self.hovered_output_idx { Some(i) => i, None => return };
        if idx >= self.outputs.len() { return; }

        let name = self.outputs[idx].name.clone().unwrap_or_else(|| format!("OUT-{idx}"));
        self.result = Some(Some(Target::OutputName(name)));
    }

    pub fn request_redraw(&mut self) {
        let any_busy = self.output_surfaces.iter().any(|os| os.shm_buf.as_ref().map_or(false, |b| b.busy));
        if any_busy {
            self.pending_redraw = true;
            return;
        }
        let _ = super::render::redraw_all(self);
    }
}

// SCTK trait implementations
impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState { &mut self.registry_state }
    registry_handlers![OutputState];
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState { &mut self.output_state }

    fn new_output(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {
        let _ = super::surfaces::try_create_surfaces(self, qh);
    }

    fn update_output(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {
        let _ = super::surfaces::try_create_surfaces(self, qh);
    }

    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

// Dispatch impls (same as yours but delegate to surfaces helpers)
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
                state.pending_redraw = true;
                state.request_redraw();
            }
            zwlr_layer_surface_v1::Event::Closed => state.cancel(),
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for App {
    fn event(state: &mut Self, pointer: &wl_pointer::WlPointer, event: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_pointer::Event::Enter { serial, surface, .. } => {
                state.set_cursor_image(pointer, serial);

                if let Some((idx, os)) = state.output_surfaces.iter().enumerate().find(|(_, os)| os.surface.id() == surface.id()) {
                    state.current_surface_idx = Some(idx);

                    if let Some(name) = os.output_info.name.as_ref() {
                        if let Some(oi) = state.outputs.iter().position(|o| o.name.as_ref() == Some(name)) {
                            state.hovered_output_idx = Some(oi);
                        }
                    }

                    state.request_redraw();
                }
            }
            wl_pointer::Event::Button { button, state: btn_state, .. } => {
                if button != BTN_LEFT { return; }
                if btn_state == WEnum::Value(wl_pointer::ButtonState::Pressed) {
                    state.confirm_hovered();
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for App {
    fn event(state: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_keyboard::Event::Key { key, state: key_state, .. } => {
                if key_state != WEnum::Value(wl_keyboard::KeyState::Pressed) { return; }
                if key == KEY_ESC { state.cancel(); }
                else if key == KEY_ENTER { state.confirm_all(); }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for App {
    fn event(state: &mut Self, buffer: &wl_buffer::WlBuffer, event: wl_buffer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_buffer::Event::Release = event {
            super::surfaces::handle_buffer_release(state, buffer);
            if state.pending_redraw {
                state.request_redraw();
            }
        }
    }
}

// empty dispatch impls you already have:
impl Dispatch<wl_compositor::WlCompositor, ()> for App { fn event(_: &mut Self, _: &wl_compositor::WlCompositor, _: wl_compositor::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<wl_shm::WlShm, ()> for App { fn event(_: &mut Self, _: &wl_shm::WlShm, _: wl_shm::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<wl_surface::WlSurface, ()> for App { fn event(_: &mut Self, _: &wl_surface::WlSurface, _: wl_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for App { fn event(_: &mut Self, _: &zwlr_layer_shell_v1::ZwlrLayerShellV1, _: zwlr_layer_shell_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }

impl Dispatch<wl_seat::WlSeat, ()> for App {
    fn event(state: &mut Self, seat: &wl_seat::WlSeat, event: wl_seat::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            if let WEnum::Value(caps) = capabilities {
                if caps.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                    state.pointer = Some(seat.get_pointer(qh, ()));
                }
                if caps.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
                    state.keyboard = Some(seat.get_keyboard(qh, ()));
                }
            }
        }
    }
}

smithay_client_toolkit::delegate_output!(App);
smithay_client_toolkit::delegate_registry!(App);
