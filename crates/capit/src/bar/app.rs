// Author: Dustin Pilgrim
// License: MIT
//
// Floating bar UI - pick mode and quit

use capit_core::Mode;

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
    Connection, Dispatch, QueueHandle, WEnum,
};

use wayland_cursor::CursorTheme;

use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1,
    zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity},
};

use super::render;
use super::shm::ShmBuffer;

const BTN_LEFT: u32 = 272;
const KEY_ESC: u32 = 1;
const KEY_ENTER: u32 = 28;

// Bar geometry
pub(crate) const BAR_W: i32 = 420;
pub(crate) const BAR_H: i32 = 80;
pub(crate) const SLOT: i32 = BAR_W / 3;
pub(crate) const BAR_MARGIN_BOTTOM: i32 = 24;
pub(crate) const RADIUS: i32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    Region,
    Screen,
    Window,
}

impl Choice {
    pub(crate) fn to_mode(self) -> Mode {
        match self {
            Choice::Region => Mode::Region,
            Choice::Screen => Mode::Screen,
            Choice::Window => Mode::Window,
        }
    }
}

pub struct App {
    // SCTK state
    pub registry_state: RegistryState,
    pub output_state: OutputState,

    // Wayland globals
    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub seat: Option<wl_seat::WlSeat>,
    pub layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,

    // Surface + buffer
    pub(crate) surface: Option<wl_surface::WlSurface>,
    pub(crate) layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    pub(crate) shm_buf: Option<ShmBuffer>,
    pub(crate) configured: bool,

    // Inputs
    pub pointer: Option<wl_pointer::WlPointer>,
    pub keyboard: Option<wl_keyboard::WlKeyboard>,

    // Cursor
    pub cursor_surface: Option<wl_surface::WlSurface>,
    pub cursor_theme: Option<CursorTheme>,
    pub cursor_name: &'static str,

    // UI state
    pub(crate) hover: Option<Choice>,
    pub(crate) selected: Option<Choice>,
    pub(crate) window_supported: bool,

    // Daemon-provided colours (ARGB)
    pub(crate) accent_colour: u32,
    pub(crate) bar_background_colour: u32,

    pub(crate) pending_redraw: bool,
    pub result: Option<Option<Mode>>,
}

impl App {
    pub fn new(
        registry_state: RegistryState,
        output_state: OutputState,
        accent_colour: u32,
        bar_background_colour: u32,
    ) -> Self {
        Self {
            registry_state,
            output_state,
            compositor: None,
            shm: None,
            seat: None,
            layer_shell: None,

            surface: None,
            layer_surface: None,
            shm_buf: None,
            configured: false,

            pointer: None,
            keyboard: None,

            cursor_surface: None,
            cursor_theme: None,
            cursor_name: "left_ptr",

            hover: None,
            selected: None,
            window_supported: false,

            accent_colour,
            bar_background_colour,

            pending_redraw: true,
            result: None,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.result.is_some()
    }

    pub fn cancel(&mut self) {
        self.result = Some(None);
    }

    pub fn confirm(&mut self) {
        let Some(ch) = self.selected.or(self.hover) else {
            return;
        };

        if ch == Choice::Window && !self.window_supported {
            return;
        }

        self.result = Some(Some(ch.to_mode()));
    }

    pub fn init_cursor(&mut self, conn: &Connection, qh: &QueueHandle<Self>) -> Result<(), String> {
        if self.cursor_theme.is_some() {
            return Ok(());
        }
        let compositor = self.compositor.as_ref().ok_or("no compositor")?;
        let shm = self.shm.as_ref().ok_or("no shm")?;

        let theme = CursorTheme::load(conn, shm.clone(), 28)
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
            None => match theme.get_cursor("left_ptr") {
                Some(c) => Some(c),
                None => theme.get_cursor("default"),
            },
        };

        let Some(cursor) = cursor else { return; };

        let img = &cursor[0];
        let (hx, hy) = img.hotspot();
        pointer.set_cursor(serial, Some(surf), hx as i32, hy as i32);

        surf.attach(Some(&**img), 0, 0);
        surf.commit();
    }

    pub fn ensure_surface(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
        if self.surface.is_some() {
            return Ok(());
        }

        let compositor = self.compositor.as_ref().ok_or("no compositor")?;
        let layer_shell = self.layer_shell.as_ref().ok_or("no layer_shell")?;
        let shm = self.shm.as_ref().ok_or("no shm")?;

        let surface = compositor.create_surface(qh, ());
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Overlay,
            "capit-bar".into(),
            qh,
            (),
        );

        // Centered at bottom
        layer_surface.set_anchor(Anchor::Bottom);
        layer_surface.set_margin(0, 0, BAR_MARGIN_BOTTOM, 0);

        // Keyboard focus so ESC/ENTER works reliably
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);

        // Don't reserve layout space (we're an overlay)
        layer_surface.set_exclusive_zone(0);

        // Request size
        layer_surface.set_size(BAR_W as u32, BAR_H as u32);

        self.shm_buf = Some(ShmBuffer::new(shm, qh, BAR_W, BAR_H)?);

        self.surface = Some(surface.clone());
        self.layer_surface = Some(layer_surface);

        surface.commit();
        Ok(())
    }

    pub fn request_redraw(&mut self) {
        let busy = self.shm_buf.as_ref().map_or(false, |b| b.busy);
        if busy || !self.configured {
            self.pending_redraw = true;
            return;
        }
        let _ = render::redraw(self);
    }

    fn hit_choice(&self, x: f64, y: f64) -> Option<Choice> {
        if y < 0.0 || y >= BAR_H as f64 {
            return None;
        }
        let xi = x as i32;
        if xi < 0 || xi >= BAR_W {
            return None;
        }
        match xi / SLOT {
            0 => Some(Choice::Region),
            1 => Some(Choice::Screen),
            2 => Some(Choice::Window),
            _ => None,
        }
    }
}

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

// Dispatch impls
impl Dispatch<wl_compositor::WlCompositor, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_shm::WlShm, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_shm_pool::WlShmPool, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_surface::WlSurface, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        _: zwlr_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for App {
    fn event(
        state: &mut Self,
        proxy: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                proxy.ack_configure(serial);

                let w = if width == 0 { BAR_W as u32 } else { width };
                let h = if height == 0 { BAR_H as u32 } else { height };

                let needs_resize = state
                    .shm_buf
                    .as_ref()
                    .map_or(true, |b| b.width != w as i32 || b.height != h as i32);

                if needs_resize {
                    if let Some(shm) = state.shm.as_ref() {
                        if let Ok(new_buf) = ShmBuffer::new(shm, qh, w as i32, h as i32) {
                            state.shm_buf = Some(new_buf);
                        }
                    }
                }

                state.configured = true;
                let _ = state.init_cursor(conn, qh);

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
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
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
            wl_pointer::Event::Enter { serial, surface_x, surface_y, .. } => {
                state.cursor_name = "left_ptr";
                state.set_cursor_image(pointer, serial);

                let h = state.hit_choice(surface_x, surface_y);
                if h != state.hover {
                    state.hover = h;
                    state.request_redraw();
                }
            }
            wl_pointer::Event::Leave { .. } => {
                if state.hover.is_some() {
                    state.hover = None;
                    state.request_redraw();
                }
            }
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                let h = state.hit_choice(surface_x, surface_y);
                if h != state.hover {
                    state.hover = h;
                    state.request_redraw();
                }
            }
            wl_pointer::Event::Button { button, state: btn_state, .. } => {
                if button != BTN_LEFT {
                    return;
                }
                if btn_state == WEnum::Value(wl_pointer::ButtonState::Pressed) {
                    if let Some(h) = state.hover {
                        if h == Choice::Window && !state.window_supported {
                            return;
                        }
                        state.selected = Some(h);
                        state.confirm(); // click confirms (returns from run_bar)
                    }
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
            if let Some(sb) = state.shm_buf.as_mut() {
                if &sb.buffer == buffer {
                    sb.busy = false;
                }
            }
            if state.pending_redraw {
                state.request_redraw();
            }
        }
    }
}

// SCTK delegates
smithay_client_toolkit::delegate_output!(App);
smithay_client_toolkit::delegate_registry!(App);
