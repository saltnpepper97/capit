// Author: Dustin Pilgrim
// License: MIT
//
// Region overlay (Wayland SHM + wlr-layer-shell) with a persistent selection rectangle
// that can be moved/resized (macOS-ish).
//
// Behavior:
// - Starts with a visible rectangle at (0,0) on the first output (we force output origin to 0,0 here).
// - LMB drag inside: move
// - LMB drag edges/corners: resize
// - ESC: cancel (returns None)
// - ENTER: confirm (returns Some(rect))
// - LMB release: ends drag (does NOT confirm)
//
// NOTE: Single-output overlay sized to OutputInfo.{width,height} (logical).

use std::fs::File;
use std::os::fd::AsFd;

use capit_core::{OutputInfo, Rect};

use memmap2::MmapMut;
use tempfile::tempfile;

use wayland_client::{
    globals::registry_queue_init,
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_pointer, wl_registry, wl_seat, wl_shm,
        wl_shm_pool, wl_surface,
    },
    Connection, Dispatch, QueueHandle, WEnum,
};

use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

const BTN_LEFT: u32 = 272; // Linux evdev BTN_LEFT
const KEY_ESC: u32 = 1; // Linux evdev KEY_ESC
const KEY_ENTER: u32 = 28; // Linux evdev KEY_ENTER

// Feel / look
const BORDER_THICKNESS: i32 = 2;

// Handles: make them a bit more grabbable again
const HANDLE_SIZE: i32 = 8; // visual size (was 6)
const HANDLE_HIT: i32 = 12; // hitbox padding around edges/corners
const MIN_W: i32 = 8;
const MIN_H: i32 = 8;

// Colors are ARGB (in u32), buffer format is ARGB8888.
const DIM_A: u8 = 0x66; // background dim alpha
const BG_DIM_ARGB: u32 = (DIM_A as u32) << 24; // black with alpha

// Inside selection MUST be transparent so you can see what's underneath.
const CLEAR_ARGB: u32 = 0x0000_0000;

// Shadow + border palette
const SHADOW_ARGB_1: u32 = 0x2A00_0000; // faint shadow
const SHADOW_ARGB_2: u32 = 0x1600_0000; // softer shadow
const BORDER_ARGB: u32 = 0xFF0A_84FF; // mac-ish blue
const BORDER_GLOW_ARGB: u32 = 0x340A_84FF; // low alpha blue glow (slightly reduced)

const HANDLE_OUTER_ARGB: u32 = BORDER_ARGB;
const HANDLE_INNER_ARGB: u32 = 0xFFFF_FFFF;

#[derive(Clone, Copy, Debug, Default)]
struct RectLocal {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl RectLocal {
    fn clamp_to(&mut self, max_w: i32, max_h: i32) {
        self.w = self.w.max(MIN_W);
        self.h = self.h.max(MIN_H);

        if self.x < 0 {
            self.x = 0;
        }
        if self.y < 0 {
            self.y = 0;
        }

        if self.x + self.w > max_w {
            self.x = (max_w - self.w).max(0);
        }
        if self.y + self.h > max_h {
            self.y = (max_h - self.h).max(0);
        }
    }

    fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < (self.x + self.w) && py < (self.y + self.h)
    }
}

#[derive(Clone, Copy, Debug)]
enum DragMode {
    None,
    Move,
    Resize(ResizeDir),
}

#[derive(Clone, Copy, Debug)]
struct ResizeDir {
    left: bool,
    right: bool,
    top: bool,
    bottom: bool,
}

impl ResizeDir {
    fn any(&self) -> bool {
        self.left || self.right || self.top || self.bottom
    }
}

struct ShmBuffer {
    _file: File,
    mmap: MmapMut,
    pool: wl_shm_pool::WlShmPool,
    buffer: wl_buffer::WlBuffer,
    width: i32,
    height: i32,
    stride: i32,
    busy: bool,
}

impl ShmBuffer {
    fn new(
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<App>,
        width: i32,
        height: i32,
    ) -> Result<Self, String> {
        let width = width.max(1);
        let height = height.max(1);
        let stride = width * 4;
        let size = (stride * height) as u64;

        let file = tempfile().map_err(|e| format!("tempfile: {e}"))?;
        file.set_len(size)
            .map_err(|e| format!("set_len({size}): {e}"))?;

        let mmap = unsafe { MmapMut::map_mut(&file).map_err(|e| format!("mmap: {e}"))? };

        let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
        let buffer = pool.create_buffer(
            0,
            width,
            height,
            stride,
            wl_shm::Format::Argb8888,
            qh,
            (),
        );

        Ok(Self {
            _file: file,
            mmap,
            pool,
            buffer,
            width,
            height,
            stride,
            busy: false,
        })
    }

    fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.mmap[..]
    }
}

pub struct App {
    output: OutputInfo,

    // Globals
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    seat: Option<wl_seat::WlSeat>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,

    // Objects
    surface: Option<wl_surface::WlSurface>,
    // (1) typo patch: correct type name
    layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,

    // SHM buffer
    shm_buf: Option<ShmBuffer>,

    // Selection state
    cursor: (i32, i32),
    selection: RectLocal,

    // Drag state
    drag_mode: DragMode,
    grab_cursor: (i32, i32),
    grab_rect: RectLocal,

    // Redraw
    pending_redraw: bool,

    // Result
    result: Option<Option<Rect>>,
}

impl App {
    fn new(mut output: OutputInfo) -> Self {
        // Force overlay coords to start from (0,0) as requested.
        output.x = 0;
        output.y = 0;

        // Start with a rectangle at (0,0).
        let init_w = (output.width / 2).clamp(260, output.width.max(1));
        let init_h = (output.height / 2).clamp(180, output.height.max(1));

        Self {
            output,
            compositor: None,
            shm: None,
            seat: None,
            layer_shell: None,

            surface: None,
            layer_surface: None,
            pointer: None,
            keyboard: None,

            shm_buf: None,

            cursor: (0, 0),
            selection: RectLocal {
                x: 0,
                y: 0,
                w: init_w.max(MIN_W),
                h: init_h.max(MIN_H),
            },

            drag_mode: DragMode::None,
            grab_cursor: (0, 0),
            grab_rect: RectLocal::default(),

            pending_redraw: true,
            result: None,
        }
    }

    fn setup(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
        let compositor = self.compositor.as_ref().ok_or("missing wl_compositor")?;
        let layer_shell = self.layer_shell.as_ref().ok_or("missing layer_shell")?;
        let shm = self.shm.as_ref().ok_or("missing wl_shm")?;

        let width = self.output.width.max(1);
        let height = self.output.height.max(1);

        let surface = compositor.create_surface(qh, ());
        self.surface = Some(surface.clone());

        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Overlay,
            "capit-region".into(),
            qh,
            (),
        );
        self.layer_surface = Some(layer_surface.clone());

        layer_surface.set_anchor(
            zwlr_layer_surface_v1::Anchor::Top
                | zwlr_layer_surface_v1::Anchor::Bottom
                | zwlr_layer_surface_v1::Anchor::Left
                | zwlr_layer_surface_v1::Anchor::Right,
        );
        layer_surface.set_keyboard_interactivity(
            zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive,
        );
        layer_surface.set_exclusive_zone(-1);
        layer_surface.set_size(0, 0);

        self.shm_buf = Some(ShmBuffer::new(shm, qh, width, height)?);

        // Kick compositor into sending Configure
        surface.commit();
        Ok(())
    }

    fn local_to_global(&self, lr: RectLocal) -> Rect {
        Rect {
            x: self.output.x + lr.x,
            y: self.output.y + lr.y,
            w: lr.w,
            h: lr.h,
        }
    }

    fn cancel(&mut self) {
        self.result = Some(None);
    }

    fn confirm(&mut self) {
        let mut r = self.selection;
        r.clamp_to(self.output.width.max(1), self.output.height.max(1));
        self.result = Some(Some(self.local_to_global(r)));
    }

    fn is_finished(&self) -> bool {
        self.result.is_some()
    }

    fn request_redraw(&mut self) {
        if let Some(sb) = self.shm_buf.as_ref() {
            if sb.busy {
                self.pending_redraw = true;
                return;
            }
        }
        let _ = self.redraw();
    }

    fn redraw(&mut self) -> Result<(), String> {
        let surface = self.surface.as_ref().ok_or("no surface")?;
        let sb = self.shm_buf.as_mut().ok_or("no shm buffer")?;
        if sb.busy {
            self.pending_redraw = true;
            return Ok(());
        }

        let buf_w = sb.width;
        let buf_h = sb.height;
        let buf = sb.pixels_mut();

        // Fullscreen dim.
        fill_u32(buf, BG_DIM_ARGB);

        // Selection (clamped).
        let mut sel = self.selection;
        sel.clamp_to(buf_w, buf_h);

        // Soft drop shadow (two passes).
        draw_border_u32(
            buf,
            buf_w,
            buf_h,
            sel.x + 2,
            sel.y + 2,
            sel.w,
            sel.h,
            BORDER_THICKNESS + 2,
            SHADOW_ARGB_2,
        );
        draw_border_u32(
            buf,
            buf_w,
            buf_h,
            sel.x + 1,
            sel.y + 1,
            sel.w,
            sel.h,
            BORDER_THICKNESS + 1,
            SHADOW_ARGB_1,
        );

        // IMPORTANT: inside selection must be transparent to see the screen.
        fill_rect_u32(buf, buf_w, buf_h, sel.x, sel.y, sel.w, sel.h, CLEAR_ARGB);

        // Subtle blue glow under the real border.
        draw_border_u32(
            buf,
            buf_w,
            buf_h,
            sel.x,
            sel.y,
            sel.w,
            sel.h,
            BORDER_THICKNESS + 2,
            BORDER_GLOW_ARGB,
        );

        // Real border.
        draw_border_u32(
            buf,
            buf_w,
            buf_h,
            sel.x,
            sel.y,
            sel.w,
            sel.h,
            BORDER_THICKNESS,
            BORDER_ARGB,
        );

        // Fake rounded corners (knock out extreme corner pixels).
        soften_corners(buf, buf_w, buf_h, sel);

        // Corner handles (bigger again).
        draw_corner_handles(buf, buf_w, buf_h, sel);

        surface.attach(Some(&sb.buffer), 0, 0);
        surface.damage_buffer(0, 0, buf_w, buf_h);
        surface.commit();

        self.pending_redraw = false;
        sb.busy = true;
        Ok(())
    }

    fn hit_test(&self, px: i32, py: i32) -> DragMode {
        let r = self.selection;

        let left = (px - r.x).abs() <= HANDLE_HIT
            && py >= r.y - HANDLE_HIT
            && py <= r.y + r.h + HANDLE_HIT;
        let right = (px - (r.x + r.w)).abs() <= HANDLE_HIT
            && py >= r.y - HANDLE_HIT
            && py <= r.y + r.h + HANDLE_HIT;
        let top = (py - r.y).abs() <= HANDLE_HIT
            && px >= r.x - HANDLE_HIT
            && px <= r.x + r.w + HANDLE_HIT;
        let bottom = (py - (r.y + r.h)).abs() <= HANDLE_HIT
            && px >= r.x - HANDLE_HIT
            && px <= r.x + r.w + HANDLE_HIT;

        let dir = ResizeDir {
            left,
            right,
            top,
            bottom,
        };

        if dir.any() {
            return DragMode::Resize(dir);
        }

        if r.contains(px, py) {
            DragMode::Move
        } else {
            DragMode::Resize(ResizeDir {
                left: false,
                right: true,
                top: false,
                bottom: true,
            })
        }
    }

    fn apply_drag(&mut self) {
        let (cx, cy) = self.cursor;
        let dx = cx - self.grab_cursor.0;
        let dy = cy - self.grab_cursor.1;

        let max_w = self.output.width.max(1);
        let max_h = self.output.height.max(1);

        match self.drag_mode {
            DragMode::None => {}
            DragMode::Move => {
                let mut r = self.grab_rect;
                r.x += dx;
                r.y += dy;
                r.clamp_to(max_w, max_h);
                self.selection = r;
            }
            DragMode::Resize(dir) => {
                let mut r = self.grab_rect;

                if dir.left {
                    r.x += dx;
                    r.w -= dx;
                }
                if dir.right {
                    r.w += dx;
                }
                if dir.top {
                    r.y += dy;
                    r.h -= dy;
                }
                if dir.bottom {
                    r.h += dy;
                }

                // Normalize if inverted.
                if r.w < 0 {
                    r.x += r.w;
                    r.w = -r.w;
                }
                if r.h < 0 {
                    r.y += r.h;
                    r.h = -r.h;
                }

                r.w = r.w.max(MIN_W);
                r.h = r.h.max(MIN_H);
                r.clamp_to(max_w, max_h);
                self.selection = r;
            }
        }
    }
}

// -------------------- Dispatch impls --------------------

impl Dispatch<wl_registry::WlRegistry, wayland_client::globals::GlobalListContents> for App {
    fn event(
        _state: &mut Self,
        _registry: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &wayland_client::globals::GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

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
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                proxy.ack_configure(serial);

                let needs_resize = if let Some(ref buf) = state.shm_buf {
                    buf.width != width as i32 || buf.height != height as i32
                } else {
                    true
                };

                if needs_resize && width > 0 && height > 0 {
                    if let Some(shm) = state.shm.as_ref() {
                        if let Ok(new_buf) =
                            ShmBuffer::new(shm, qh, width as i32, height as i32)
                        {
                            state.shm_buf = Some(new_buf);
                        }
                    }
                }

                state.selection.clamp_to(width as i32, height as i32);
                state.pending_redraw = true;
                state.request_redraw();
            }
            zwlr_layer_surface_v1::Event::Closed => {
                state.cancel();
            }
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
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { surface_x, surface_y, .. } => {
                state.cursor = (surface_x as i32, surface_y as i32);
                state.request_redraw();
            }
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                state.cursor = (surface_x as i32, surface_y as i32);
                if !matches!(state.drag_mode, DragMode::None) {
                    state.apply_drag();
                }
                state.request_redraw();
            }
            wl_pointer::Event::Button { button, state: btn_state, .. } => {
                if button != BTN_LEFT {
                    return;
                }
                match btn_state {
                    WEnum::Value(wl_pointer::ButtonState::Pressed) => {
                        state.grab_cursor = state.cursor;
                        state.grab_rect = state.selection;
                        state.drag_mode = state.hit_test(state.cursor.0, state.cursor.1);

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
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            if let Some(sb) = state.shm_buf.as_mut() {
                sb.busy = false;
            }
            if state.pending_redraw {
                state.request_redraw();
            }
        }
    }
}

// -------------------- Public entrypoint --------------------

pub fn run_region_overlay(mut output: OutputInfo) -> Result<Option<Rect>, String> {
    output.x = 0;
    output.y = 0;

    let conn = Connection::connect_to_env().map_err(|e| format!("wayland connect: {e}"))?;
    let (globals, mut queue) =
        registry_queue_init::<App>(&conn).map_err(|e| format!("registry init: {e}"))?;

    let qh = queue.handle();
    let mut app = App::new(output);

    app.compositor = globals
        .bind::<wl_compositor::WlCompositor, _, _>(&qh, 1..=6, ())
        .ok();
    app.shm = globals.bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ()).ok();
    app.seat = globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=7, ()).ok();
    app.layer_shell = globals
        .bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(&qh, 1..=4, ())
        .ok();

    queue
        .roundtrip(&mut app)
        .map_err(|e| format!("roundtrip: {e}"))?;

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

    app.setup(&qh)?;

    while !app.is_finished() {
        queue
            .blocking_dispatch(&mut app)
            .map_err(|e| format!("dispatch: {e}"))?;
        let _ = conn.flush();
    }

    Ok(app.result.unwrap_or(None))
}

// -------------------- Pixel helpers (fast u32 path) --------------------

fn fill_u32(buf: &mut [u8], argb: u32) {
    let (head, body, tail) = unsafe { buf.align_to_mut::<u32>() };
    debug_assert!(head.is_empty() && tail.is_empty());
    body.fill(argb);
}

fn fill_rect_u32(
    buf: &mut [u8],
    w: i32,
    h: i32,
    x: i32,
    y: i32,
    rw: i32,
    rh: i32,
    argb: u32,
) {
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + rw).min(w);
    let y1 = (y + rh).min(h);
    if x1 <= x0 || y1 <= y0 {
        return;
    }

    let (head, body, tail) = unsafe { buf.align_to_mut::<u32>() };
    debug_assert!(head.is_empty() && tail.is_empty());
    let bw = w as usize;

    for yy in y0..y1 {
        let row = yy as usize * bw;
        let start = row + x0 as usize;
        let end = row + x1 as usize;
        body[start..end].fill(argb);
    }
}

fn draw_border_u32(
    buf: &mut [u8],
    w: i32,
    h: i32,
    x: i32,
    y: i32,
    rw: i32,
    rh: i32,
    t: i32,
    argb: u32,
) {
    if rw <= 0 || rh <= 0 || t <= 0 {
        return;
    }
    fill_rect_u32(buf, w, h, x, y, rw, t, argb); // top
    fill_rect_u32(buf, w, h, x, y + rh - t, rw, t, argb); // bottom
    fill_rect_u32(buf, w, h, x, y, t, rh, argb); // left
    fill_rect_u32(buf, w, h, x + rw - t, y, t, rh, argb); // right
}

fn soften_corners(buf: &mut [u8], w: i32, h: i32, r: RectLocal) {
    let bg = BG_DIM_ARGB;

    // TL
    fill_rect_u32(buf, w, h, r.x, r.y, 1, 1, bg);
    fill_rect_u32(buf, w, h, r.x + 1, r.y, 1, 1, bg);
    fill_rect_u32(buf, w, h, r.x, r.y + 1, 1, 1, bg);

    // TR
    fill_rect_u32(buf, w, h, r.x + r.w - 1, r.y, 1, 1, bg);
    fill_rect_u32(buf, w, h, r.x + r.w - 2, r.y, 1, 1, bg);
    fill_rect_u32(buf, w, h, r.x + r.w - 1, r.y + 1, 1, 1, bg);

    // BL
    fill_rect_u32(buf, w, h, r.x, r.y + r.h - 1, 1, 1, bg);
    fill_rect_u32(buf, w, h, r.x + 1, r.y + r.h - 1, 1, 1, bg);
    fill_rect_u32(buf, w, h, r.x, r.y + r.h - 2, 1, 1, bg);

    // BR
    fill_rect_u32(buf, w, h, r.x + r.w - 1, r.y + r.h - 1, 1, 1, bg);
    fill_rect_u32(buf, w, h, r.x + r.w - 2, r.y + r.h - 1, 1, 1, bg);
    fill_rect_u32(buf, w, h, r.x + r.w - 1, r.y + r.h - 2, 1, 1, bg);
}

fn draw_corner_handles(buf: &mut [u8], w: i32, h: i32, r: RectLocal) {
    let hs = HANDLE_SIZE;
    let half = hs / 2;

    // Center the handle on the exact corner.
    let tl = (r.x - half, r.y - half);
    let tr = (r.x + r.w - half, r.y - half);
    let bl = (r.x - half, r.y + r.h - half);
    let br = (r.x + r.w - half, r.y + r.h - half);

    draw_handle(buf, w, h, tl.0, tl.1);
    draw_handle(buf, w, h, tr.0, tr.1);
    draw_handle(buf, w, h, bl.0, bl.1);
    draw_handle(buf, w, h, br.0, br.1);
}

fn draw_handle(buf: &mut [u8], w: i32, h: i32, x: i32, y: i32) {
    // Outer blue square
    fill_rect_u32(buf, w, h, x, y, HANDLE_SIZE, HANDLE_SIZE, HANDLE_OUTER_ARGB);

    // Inner white dot (2x2) centered
    let inner = 2;
    let ix = x + (HANDLE_SIZE - inner) / 2;
    let iy = y + (HANDLE_SIZE - inner) / 2;
    fill_rect_u32(buf, w, h, ix, iy, inner, inner, HANDLE_INNER_ARGB);
}
