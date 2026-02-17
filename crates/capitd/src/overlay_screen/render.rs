// Author: Dustin Pilgrim
// License: MIT

use super::app::App;

const DIM_A: u8 = 0x88;
const HOVER_DIM_A: u8 = 0x44;
const BG_DIM_ARGB: u32 = (DIM_A as u32) << 24;
const HOVER_DIM_ARGB: u32 = (HOVER_DIM_A as u32) << 24;

const BORDER_THICKNESS: i32 = 2;

pub fn redraw_all(app: &mut App) -> Result<(), String> {
    let hovered_name = app
        .hovered_output_idx
        .and_then(|i| app.outputs.get(i))
        .and_then(|o| o.name.as_ref())
        .cloned();

    let border_argb: u32 = app.accent_colour;
    let border_glow_argb: u32 = (border_argb & 0x00FF_FFFF) | (0x34u32 << 24);

    for (si, os) in app.output_surfaces.iter_mut().enumerate() {
        if !os.configured {
            continue;
        }

        let sb = os.shm_buf.as_mut().ok_or("no shm buffer")?;
        if sb.busy {
            app.pending_redraw = true;
            continue;
        }

        let buf_w = sb.width;
        let buf_h = sb.height;
        let buf = sb.pixels_mut();

        let is_hovered = match (&hovered_name, os.output_info.name.as_ref()) {
            (Some(h), Some(n)) => h == n,
            _ => app.current_surface_idx == Some(si),
        };

        if is_hovered {
            fill_u32(buf, HOVER_DIM_ARGB);
            draw_border_u32(
                buf,
                buf_w,
                buf_h,
                1,
                1,
                buf_w - 2,
                buf_h - 2,
                BORDER_THICKNESS + 2,
                border_glow_argb,
            );
            draw_border_u32(
                buf,
                buf_w,
                buf_h,
                2,
                2,
                buf_w - 4,
                buf_h - 4,
                BORDER_THICKNESS,
                border_argb,
            );
        } else {
            fill_u32(buf, BG_DIM_ARGB);
        }

        os.surface.attach(Some(&sb.buffer), 0, 0);
        os.surface.damage_buffer(0, 0, buf_w, buf_h);
        os.surface.commit();
        sb.busy = true;
    }

    app.pending_redraw = false;
    Ok(())
}

// pixel helpers
fn fill_u32(buf: &mut [u8], argb: u32) {
    let (_, body, _) = unsafe { buf.align_to_mut::<u32>() };
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

    let (_, body, _) = unsafe { buf.align_to_mut::<u32>() };
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
    fill_rect_u32(buf, w, h, x, y, rw, t, argb);
    fill_rect_u32(buf, w, h, x, y + rh - t, rw, t, argb);
    fill_rect_u32(buf, w, h, x, y, t, rh, argb);
    fill_rect_u32(buf, w, h, x + rw - t, y, t, rh, argb);
}
