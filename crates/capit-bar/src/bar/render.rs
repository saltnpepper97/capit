// Author: Dustin Pilgrim
// License: MIT

use super::app::{App, Choice, BAR_H, BAR_W, RADIUS, SLOT};
use super::icons::{icons, ICON_SZ};
use super::pixels;

// Flat UI colours (solid, no weird translucency)
const BAR_BORDER: u32 = 0xFF2A_2E36;

const BTN_IDLE: u32 = 0xFF16_1920;
const BTN_HOVER: u32 = 0xFF1E_2330;
const BTN_SELECTED: u32 = 0xFF2A_313C;
const BTN_DISABLED: u32 = 0xFF12_1418;

const ICON_TINT_ON: u32 = 0xFFF5_F7FA;
const ICON_TINT_OFF: u32 = 0xFF6B_7078;

const BTN_PAD: i32 = 10;

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

    // Solid bar background + border
    pixels::fill_u32(buf, app.bar_background_colour);
    pixels::draw_rect_outline(buf, BAR_W, BAR_H, 0, 0, BAR_W, BAR_H, 1, BAR_BORDER);

    // Rounded bar shape
    pixels::apply_rounded_mask(buf, BAR_W, BAR_H, RADIUS);

    // Slots (solid buttons)
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
) {
    let bg = if !enabled {
        BTN_DISABLED
    } else if selected {
        BTN_SELECTED
    } else if hovered {
        BTN_HOVER
    } else {
        BTN_IDLE
    };

    let x = slot_x + BTN_PAD;
    let y = BTN_PAD;
    let rw = slot_w - BTN_PAD * 2;
    let rh = slot_h - BTN_PAD * 2;

    pixels::fill_rect_u32(buf, w, h, x, y, rw, rh, bg);

    // Use accent colour for selected border so the bar clearly reflects daemon theme.
    let border = if !enabled {
        0xFF1A_1D24
    } else if selected {
        accent_colour
    } else if hovered {
        0xFF3A_4250
    } else {
        BAR_BORDER
    };

    pixels::draw_rect_outline(buf, w, h, x, y, rw, rh, 1, border);
}
