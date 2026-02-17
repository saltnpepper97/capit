// Author: Dustin Pilgrim
// License: MIT

use super::app::App;
use super::model::{RectLocal, BORDER_THICKNESS};
use super::pixels::*;

const DIM_A: u8 = 0x66;
const BG_DIM_ARGB: u32 = (DIM_A as u32) << 24;
const CLEAR_ARGB: u32 = 0x0000_0000;
const SHADOW_ARGB_1: u32 = 0x2A00_0000;
const SHADOW_ARGB_2: u32 = 0x1600_0000;
const HANDLE_INNER_ARGB: u32 = 0xFFFF_FFFF;

pub fn redraw_all(app: &mut App) -> Result<(), String> {
    // Use daemon-provided accent colour for border + handles
    let border_argb: u32 = app.accent_colour;
    let handle_outer_argb: u32 = border_argb;

    for output_surface in &mut app.output_surfaces {
        if !output_surface.configured {
            continue;
        }

        let sb = output_surface.shm_buf.as_mut().ok_or("no shm buffer")?;
        if sb.busy {
            app.pending_redraw = true;
            continue;
        }

        let buf_w = sb.width;
        let buf_h = sb.height;
        let buf = sb.pixels_mut();

        let output_info = &output_surface.output_info;

        // Convert selection to output-local coords
        let sel_local = RectLocal {
            x: app.selection.x - output_info.x,
            y: app.selection.y - output_info.y,
            w: app.selection.w,
            h: app.selection.h,
        };

        let sel_right = sel_local.x + sel_local.w;
        let sel_bottom = sel_local.y + sel_local.h;

        let intersects =
            sel_right > 0 && sel_local.x < buf_w && sel_bottom > 0 && sel_local.y < buf_h;

        if intersects {
            fill_u32(buf, BG_DIM_ARGB);

            let sel = sel_local;
            let clip_x = sel.x.max(0);
            let clip_y = sel.y.max(0);
            let clip_w = (sel.x + sel.w).min(buf_w) - clip_x;
            let clip_h = (sel.y + sel.h).min(buf_h) - clip_y;

            if clip_w > 0 && clip_h > 0 {
                let mostly_visible = sel.x >= -20
                    && sel.y >= -20
                    && sel.x + sel.w <= buf_w + 20
                    && sel.y + sel.h <= buf_h + 20;

                if mostly_visible {
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

                    fill_rect_u32(buf, buf_w, buf_h, clip_x, clip_y, clip_w, clip_h, CLEAR_ARGB);

                    draw_border_u32(
                        buf,
                        buf_w,
                        buf_h,
                        sel.x,
                        sel.y,
                        sel.w,
                        sel.h,
                        BORDER_THICKNESS,
                        border_argb,
                    );

                    soften_corners(buf, buf_w, buf_h, sel, BG_DIM_ARGB);
                    draw_corner_handles(
                        buf,
                        buf_w,
                        buf_h,
                        sel,
                        handle_outer_argb,
                        HANDLE_INNER_ARGB,
                    );
                } else {
                    fill_rect_u32(buf, buf_w, buf_h, clip_x, clip_y, clip_w, clip_h, CLEAR_ARGB);

                    draw_border_u32(
                        buf,
                        buf_w,
                        buf_h,
                        sel.x,
                        sel.y,
                        sel.w,
                        sel.h,
                        BORDER_THICKNESS,
                        border_argb,
                    );
                }
            }
        } else {
            fill_u32(buf, BG_DIM_ARGB);
        }

        output_surface.surface.attach(Some(&sb.buffer), 0, 0);
        output_surface.surface.damage_buffer(0, 0, buf_w, buf_h);
        output_surface.surface.commit();
        sb.busy = true;
    }

    app.pending_redraw = false;
    Ok(())
}
