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

pub fn draw_corner_handles(
    buf: &mut [u8],
    w: i32,
    h: i32,
    r: RectLocal,
    outer: u32,
    inner: u32,
) {
    let half = HANDLE_SIZE / 2;
    let handles = [
        (r.x - half, r.y - half),
        (r.x + r.w - half, r.y - half),
        (r.x - half, r.y + r.h - half),
        (r.x + r.w - half, r.y + r.h - half),
    ];

    for &(x, y) in &handles {
        draw_handle(buf, w, h, x, y, outer, inner);
    }
}

pub fn draw_handle(buf: &mut [u8], w: i32, h: i32, x: i32, y: i32, outer: u32, inner: u32) {
    fill_rect_u32(buf, w, h, x, y, HANDLE_SIZE, HANDLE_SIZE, outer);
    let inner_sz = 2;
    let ix = x + (HANDLE_SIZE - inner_sz) / 2;
    let iy = y + (HANDLE_SIZE - inner_sz) / 2;
    fill_rect_u32(buf, w, h, ix, iy, inner_sz, inner_sz, inner);
}
