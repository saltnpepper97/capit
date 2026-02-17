// Author: Dustin Pilgrim
// License: MIT

use super::app::{App, Choice, BAR_H, BAR_W, RADIUS, SLOT};
use super::colour;
use super::icons::{icons, ICON_SZ};
use super::pixels;

// Icon tints
const ICON_TINT_ON: u32 = 0xFFF5_F7FA;
const ICON_TINT_OFF: u32 = 0xFF6B_7078;

// Layout
const BTN_PAD: i32 = 10;

// Disabled slash (derived alpha + RGB from ICON_TINT_OFF)
const DISABLED_SLASH_ALPHA: u8 = 0xD0; // a bit softer than your 0xCC, reads nicer on many bgs

pub(crate) fn redraw(app: &mut App) -> Result<(), String> {
    if !app.configured {
        app.pending_redraw = true;
        return Ok(());
    }

    let sb = app.shm_buf.as_mut().ok_or("no shm buffer")?;
    if sb.busy {
        app.pending_redraw = true;
        return Ok(());
    }

    let buf = sb.pixels_mut();

    // Derive slot colours from bar background (single config knob stays clean).
    let sc = colour::derive_slot_colours(app.bar_background_colour);

    // Bar background + subtle border derived from bg
    pixels::fill_u32(buf, app.bar_background_colour);
    pixels::draw_rect_outline(buf, BAR_W, BAR_H, 0, 0, BAR_W, BAR_H, 1, sc.border);

    // Rounded bar shape
    pixels::apply_rounded_mask(buf, BAR_W, BAR_H, RADIUS);

    // Slots
    for i in 0..3 {
        let x = i * SLOT;
        let (choice, enabled) = match i {
            0 => (Choice::Region, true),
            1 => (Choice::Screen, true),
            _ => (Choice::Window, app.window_supported),
        };

        let selected = app.selected == Some(choice);
        let hovered = app.hover == Some(choice);

        draw_slot(
            buf,
            BAR_W,
            BAR_H,
            x,
            0,
            SLOT,
            BAR_H,
            selected,
            hovered,
            enabled,
            app.accent_colour,
            sc,
        );
    }

    // Icons (rendered from SVG once, then blitted as alpha mask)
    let y0 = (BAR_H - ICON_SZ) / 2;
    let icon_x0 = 0 * SLOT + (SLOT - ICON_SZ) / 2;
    let icon_x1 = 1 * SLOT + (SLOT - ICON_SZ) / 2;
    let icon_x2 = 2 * SLOT + (SLOT - ICON_SZ) / 2;

    let ic = icons();

    // tint = accent when hovered OR selected, otherwise white (or disabled grey)
    let region_active = app.selected == Some(Choice::Region) || app.hover == Some(Choice::Region);
    let screen_active = app.selected == Some(Choice::Screen) || app.hover == Some(Choice::Screen);
    let window_active = app.selected == Some(Choice::Window) || app.hover == Some(Choice::Window);

    let accent = app.accent_colour;

    let region_tint = if region_active { accent } else { ICON_TINT_ON };
    let screen_tint = if screen_active { accent } else { ICON_TINT_ON };

    let window_tint = if !app.window_supported {
        ICON_TINT_OFF
    } else if window_active {
        accent
    } else {
        ICON_TINT_ON
    };

    pixels::blit_alpha_tinted(buf, BAR_W, BAR_H, icon_x0, y0, ICON_SZ, &ic.region, region_tint);
    pixels::blit_alpha_tinted(buf, BAR_W, BAR_H, icon_x1, y0, ICON_SZ, &ic.screen, screen_tint);
    pixels::blit_alpha_tinted(buf, BAR_W, BAR_H, icon_x2, y0, ICON_SZ, &ic.window, window_tint);

    let surface = app.surface.as_ref().ok_or("no surface")?;
    surface.attach(Some(&sb.buffer), 0, 0);
    surface.damage_buffer(0, 0, BAR_W, BAR_H);
    surface.commit();
    sb.busy = true;

    app.pending_redraw = false;
    Ok(())
}

fn draw_slot(
    buf: &mut [u8],
    w: i32,
    h: i32,
    slot_x: i32,
    _slot_y: i32,
    slot_w: i32,
    slot_h: i32,
    selected: bool,
    hovered: bool,
    enabled: bool,
    accent_colour: u32,
    sc: colour::SlotColours,
) {
    let bg = if !enabled {
        sc.disabled
    } else if selected {
        sc.selected
    } else if hovered {
        sc.hover
    } else {
        sc.idle
    };

    let x = slot_x + BTN_PAD;
    let y = BTN_PAD;
    let rw = slot_w - BTN_PAD * 2;
    let rh = slot_h - BTN_PAD * 2;

    pixels::fill_rect_u32(buf, w, h, x, y, rw, rh, bg);

    // Outline: selected gets accent, hover gets slightly brighter than border.
    let border = if !enabled {
        sc.disabled_border
    } else if selected {
        accent_colour
    } else if hovered {
        colour::lighten(sc.border, 22)
    } else {
        sc.border
    };

    pixels::draw_rect_outline(buf, w, h, x, y, rw, rh, 1, border);

    // Extra affordance: a clean, anti-aliased "nope" slash for disabled slots.
    if !enabled {
        let slash = colour::with_alpha(ICON_TINT_OFF, DISABLED_SLASH_ALPHA);
        draw_disabled_slash_aa(buf, w, h, x, y, rw, rh, slash);
    }
}

/// Nice-looking slash that won’t look chopped:
/// - anti-aliased line (Xiaolin Wu)
/// - slight inset so it avoids borders
/// - two passes to give it a tiny "thickness" without harsh blocks
fn draw_disabled_slash_aa(buf: &mut [u8], w: i32, h: i32, x: i32, y: i32, rw: i32, rh: i32, argb: u32) {
    if rw <= 0 || rh <= 0 { return; }

    let inset = 5; // keeps it away from border corners
    let x0 = (x + inset) as f32;
    let y0 = (y + rh - 1 - inset) as f32;
    let x1 = (x + rw - 1 - inset) as f32;
    let y1 = (y + inset) as f32;

    // single anti-aliased line (no "double" look)
    wu_line(buf, w, h, x0, y0, x1, y1, argb);
}

/// Xiaolin Wu anti-aliased line (ARGB over existing pixels).
fn wu_line(buf: &mut [u8], w: i32, h: i32, mut x0: f32, mut y0: f32, mut x1: f32, mut y1: f32, argb: u32) {
    // Standard Wu needs steep handling
    let mut steep = (y1 - y0).abs() > (x1 - x0).abs();
    if steep {
        std::mem::swap(&mut x0, &mut y0);
        std::mem::swap(&mut x1, &mut y1);
    }
    if x0 > x1 {
        std::mem::swap(&mut x0, &mut x1);
        std::mem::swap(&mut y0, &mut y1);
    }

    let dx = x1 - x0;
    let dy = y1 - y0;
    let gradient = if dx.abs() < f32::EPSILON { 0.0 } else { dy / dx };

    let a = colour::a(argb) as f32 / 255.0;
    let rr = colour::r(argb);
    let gg = colour::g(argb);
    let bb = colour::b(argb);

    let x_start = x0.floor() as i32;
    let x_end = x1.floor() as i32;

    let mut y = y0;
    for x in x_start..=x_end {
        let yf = y.floor();
        let frac = y - yf;

        let a0 = (a * (1.0 - frac)).clamp(0.0, 1.0);
        let a1 = (a * frac).clamp(0.0, 1.0);

        let y_i = yf as i32;

        if steep {
            plot_over(buf, w, h, y_i, x, a0, rr, gg, bb);
            plot_over(buf, w, h, y_i + 1, x, a1, rr, gg, bb);
        } else {
            plot_over(buf, w, h, x, y_i, a0, rr, gg, bb);
            plot_over(buf, w, h, x, y_i + 1, a1, rr, gg, bb);
        }

        y += gradient;
    }
}

/// Alpha-over blend into a single pixel in the shm buffer.
/// NOTE: buffer layout here is little-endian BGRA bytes (matches your existing blits).
fn plot_over(buf: &mut [u8], w: i32, h: i32, x: i32, y: i32, alpha01: f32, r: u8, g: u8, b: u8) {
    if x < 0 || y < 0 || x >= w || y >= h { return; }
    if alpha01 <= 0.0 { return; }

    let a = (alpha01 * 255.0).round().clamp(0.0, 255.0) as u32;
    if a == 0 { return; }

    let inv = 255 - a;

    let idx = ((y * w + x) * 4) as usize;

    // dst is BGRA bytes
    let db = buf[idx] as u32;
    let dg = buf[idx + 1] as u32;
    let dr = buf[idx + 2] as u32;

    let ob = ((b as u32 * a + db * inv + 127) / 255) as u8;
    let og = ((g as u32 * a + dg * inv + 127) / 255) as u8;
    let or = ((r as u32 * a + dr * inv + 127) / 255) as u8;

    buf[idx] = ob;
    buf[idx + 1] = og;
    buf[idx + 2] = or;
    buf[idx + 3] = 255;
}
