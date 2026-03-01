// Author: Dustin Pilgrim
// License: MIT

use super::model::{RectLocal, HANDLE_SIZE};

pub fn fill_u32(buf: &mut [u8], argb: u32) {
    let (_, body, _) = unsafe { buf.align_to_mut::<u32>() };
    body.fill(argb);
}

pub fn fill_rect_u32(
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

pub fn draw_border_u32(
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

/// Draw a dashed border rectangle.
///
/// - `dash_len` and `gap_len` are in pixels.
/// - `phase` shifts the pattern along the perimeter (can be animated later).
/// - Uses simple rectangular segments (fast), no anti-aliasing.
pub fn draw_dashed_border_u32(
    buf: &mut [u8],
    w: i32,
    h: i32,
    x: i32,
    y: i32,
    rw: i32,
    rh: i32,
    t: i32,
    argb: u32,
    dash_len: i32,
    gap_len: i32,
    phase: i32,
) {
    if rw <= 0 || rh <= 0 || t <= 0 {
        return;
    }
    if dash_len <= 0 {
        // fallback to solid
        draw_border_u32(buf, w, h, x, y, rw, rh, t, argb);
        return;
    }
    let gap = gap_len.max(0);
    let period = dash_len + gap;
    if period <= 0 {
        draw_border_u32(buf, w, h, x, y, rw, rh, t, argb);
        return;
    }

    // The "run" lengths along each side where dashes live.
    // We include the full span (including corners) for a crisp box look.
    let top_len = rw;
    let right_len = rh;
    let bottom_len = rw;
    let left_len = rh;

    // Normalize phase into [0, period)
    let mut p = phase % period;
    if p < 0 {
        p += period;
    }

    // Helper: fill a 1D dashed line mapped to rect segments.
    // `emit(seg_start, seg_len)` is called for each dash segment in [0, line_len).
    fn dashed_segments<F: FnMut(i32, i32)>(
        line_len: i32,
        dash_len: i32,
        gap_len: i32,
        phase: i32,
        mut emit: F,
    ) {
        if line_len <= 0 {
            return;
        }
        let gap = gap_len.max(0);
        let period = dash_len + gap;
        if period <= 0 {
            emit(0, line_len);
            return;
        }

        // We iterate "pattern time" along the line.
        // Start at -phase so phase shifts the pattern forward.
        let mut pos = -phase;

        // Bring pos into a range where we can begin emitting segments
        // without missing any that intersect [0, line_len).
        // We just step forward until pos + dash/gap spans past 0.
        while pos + period < 0 {
            pos += period;
        }

        while pos < line_len {
            let dash_start = pos.max(0);
            let dash_end = (pos + dash_len).min(line_len);
            if dash_end > dash_start {
                emit(dash_start, dash_end - dash_start);
            }
            pos += period;
        }
    }

    // TOP: left->right at y
    dashed_segments(top_len, dash_len, gap, p, |sx, slen| {
        fill_rect_u32(buf, w, h, x + sx, y, slen, t, argb);
    });

    // RIGHT: top->bottom at x+rw-t (thickness inward)
    // Advance phase by top_len so pattern continues around the perimeter.
    let p_right = (p + top_len).rem_euclid(period);
    dashed_segments(right_len, dash_len, gap, p_right, |sy, slen| {
        fill_rect_u32(buf, w, h, x + rw - t, y + sy, t, slen, argb);
    });

    // BOTTOM: right->left at y+rh-t
    // We keep continuity by advancing phase by top_len + right_len,
    // but draw direction reversed by mapping segment positions.
    let p_bottom = (p + top_len + right_len).rem_euclid(period);
    dashed_segments(bottom_len, dash_len, gap, p_bottom, |sx, slen| {
        // sx is from 0..rw left->right; bottom runs right->left, so invert.
        let inv_start = (bottom_len - (sx + slen)).max(0);
        fill_rect_u32(buf, w, h, x + inv_start, y + rh - t, slen, t, argb);
    });

    // LEFT: bottom->top at x
    let p_left = (p + top_len + right_len + bottom_len).rem_euclid(period);
    dashed_segments(left_len, dash_len, gap, p_left, |sy, slen| {
        // sy is from 0..rh top->bottom; left runs bottom->top, so invert.
        let inv_start = (left_len - (sy + slen)).max(0);
        fill_rect_u32(buf, w, h, x, y + inv_start, t, slen, argb);
    });
}

pub fn soften_corners(buf: &mut [u8], w: i32, h: i32, r: RectLocal, bg: u32) {
    fill_rect_u32(buf, w, h, r.x, r.y, 2, 1, bg);
    fill_rect_u32(buf, w, h, r.x, r.y + 1, 1, 1, bg);
    fill_rect_u32(buf, w, h, r.x + r.w - 2, r.y, 2, 1, bg);
    fill_rect_u32(buf, w, h, r.x + r.w - 1, r.y + 1, 1, 1, bg);
    fill_rect_u32(buf, w, h, r.x, r.y + r.h - 1, 2, 1, bg);
    fill_rect_u32(buf, w, h, r.x, r.y + r.h - 2, 1, 1, bg);
    fill_rect_u32(buf, w, h, r.x + r.w - 2, r.y + r.h - 1, 2, 1, bg);
    fill_rect_u32(buf, w, h, r.x + r.w - 1, r.y + r.h - 2, 1, 1, bg);
}

#[inline]
fn blend_over(dst: u32, src: u32, src_a: u8) -> u32 {
    // Straight alpha "src over dst"
    if src_a == 0 {
        return dst;
    }
    if src_a == 255 {
        return src;
    }

    let da = (dst >> 24) as u8;
    let dr = (dst >> 16) as u8;
    let dg = (dst >> 8) as u8;
    let db = (dst >> 0) as u8;

    let sa0 = (src >> 24) as u8;
    let sr = (src >> 16) as u8;
    let sg = (src >> 8) as u8;
    let sb = (src >> 0) as u8;

    // combine provided alpha with src's own alpha
    let sa = ((sa0 as u16 * src_a as u16) / 255) as u8;

    let inv = 255u16 - sa as u16;

    let oa = (sa as u16 + (da as u16 * inv) / 255) as u8;
    let or = ((sr as u16 * sa as u16 + dr as u16 * inv) / 255) as u8;
    let og = ((sg as u16 * sa as u16 + dg as u16 * inv) / 255) as u8;
    let ob = ((sb as u16 * sa as u16 + db as u16 * inv) / 255) as u8;

    ((oa as u32) << 24) | ((or as u32) << 16) | ((og as u32) << 8) | (ob as u32)
}

fn fill_circle_aa_u32(buf: &mut [u8], w: i32, h: i32, cx: i32, cy: i32, r: i32, argb: u32) {
    if r <= 0 || w <= 0 || h <= 0 {
        return;
    }

    let (_, body, _) = unsafe { buf.align_to_mut::<u32>() };
    let bw = w as usize;

    // 1px feather for smoothing
    let rr = r as f32;
    let feather = 1.0f32;
    let r_outer = rr + feather;
    let r_inner = (rr - feather).max(0.0);

    let x0 = (cx - r - 2).max(0);
    let x1 = (cx + r + 2).min(w - 1);
    let y0 = (cy - r - 2).max(0);
    let y1 = (cy + r + 2).min(h - 1);

    for yy in y0..=y1 {
        let dy = (yy - cy) as f32;
        let row = yy as usize * bw;
        for xx in x0..=x1 {
            let dx = (xx - cx) as f32;
            let d = (dx * dx + dy * dy).sqrt();

            let a = if d <= r_inner {
                255u8
            } else if d >= r_outer {
                0u8
            } else {
                // linear falloff in the feather band
                let t = (r_outer - d) / (r_outer - r_inner); // 0..1
                (t.clamp(0.0, 1.0) * 255.0) as u8
            };

            if a != 0 {
                let idx = row + xx as usize;
                body[idx] = blend_over(body[idx], argb, a);
            }
        }
    }
}

pub fn draw_corner_handles(
    buf: &mut [u8],
    w: i32,
    h: i32,
    r: RectLocal,
    outer: u32,
    inner: u32,
) {
    let handles = [
        (r.x, r.y),
        (r.x + r.w, r.y),
        (r.x, r.y + r.h),
        (r.x + r.w, r.y + r.h),
    ];

    for &(cx, cy) in &handles {
        draw_handle(buf, w, h, cx, cy, outer, inner);
    }
}

// Smooth circular handle centered at (cx, cy).
// Kept signature stable; we intentionally draw solid (inner unused).
pub fn draw_handle(buf: &mut [u8], w: i32, h: i32, cx: i32, cy: i32, outer: u32, _inner: u32) {
    let rad = (HANDLE_SIZE / 2).max(2);
    fill_circle_aa_u32(buf, w, h, cx, cy, rad, outer);
}
