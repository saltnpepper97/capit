// Author: Dustin Pilgrim
// License: MIT

pub(crate) fn fill_u32(buf: &mut [u8], argb: u32) {
    let (_, body, _) = unsafe { buf.align_to_mut::<u32>() };
    body.fill(argb);
}

pub(crate) fn fill_rect_u32(
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

pub(crate) fn draw_rect_outline(
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
    fill_rect_u32(buf, w, h, x, y, rw, t, argb);
    fill_rect_u32(buf, w, h, x, y + rh - t, rw, t, argb);
    fill_rect_u32(buf, w, h, x, y, t, rh, argb);
    fill_rect_u32(buf, w, h, x + rw - t, y, t, rh, argb);
}

pub(crate) fn apply_rounded_mask(buf: &mut [u8], w: i32, h: i32, r: i32) {
    if r <= 0 {
        return;
    }
    let (_, body, _) = unsafe { buf.align_to_mut::<u32>() };
    let bw = w as usize;

    for cy in 0..r {
        for cx in 0..r {
            let dx = r - 1 - cx;
            let dy = r - 1 - cy;
            if dx * dx + dy * dy >= r * r {
                body[cy as usize * bw + cx as usize] = 0;
                body[cy as usize * bw + (w - 1 - cx) as usize] = 0;
                body[(h - 1 - cy) as usize * bw + cx as usize] = 0;
                body[(h - 1 - cy) as usize * bw + (w - 1 - cx) as usize] = 0;
            }
        }
    }
}

pub(crate) fn blend_over(dst: &mut u32, src: u32, src_a: u8) {
    if src_a == 0 {
        return;
    }
    if src_a == 255 {
        *dst = src;
        return;
    }

    let da = ((*dst >> 24) & 0xFF) as u32;
    let dr = ((*dst >> 16) & 0xFF) as u32;
    let dg = ((*dst >> 8) & 0xFF) as u32;
    let db = (*dst & 0xFF) as u32;

    let sa = src_a as u32;
    let sr = ((src >> 16) & 0xFF) as u32;
    let sg = ((src >> 8) & 0xFF) as u32;
    let sb = (src & 0xFF) as u32;

    let inv = 255 - sa;

    let oa = (sa + (da * inv + 127) / 255).min(255);
    let or = (sr * sa + dr * inv + 127) / 255;
    let og = (sg * sa + dg * inv + 127) / 255;
    let ob = (sb * sa + db * inv + 127) / 255;

    *dst = (oa << 24) | (or << 16) | (og << 8) | ob;
}

pub(crate) fn blit_alpha_tinted(
    buf: &mut [u8],
    w: i32,
    h: i32,
    x: i32,
    y: i32,
    icon_sz: i32,
    mask: &[u8],
    tint: u32,
) {
    let (_, body, _) = unsafe { buf.align_to_mut::<u32>() };
    let bw = w as usize;

    for iy in 0..icon_sz {
        let yy = y + iy;
        if yy < 0 || yy >= h {
            continue;
        }
        let row_off = yy as usize * bw;

        for ix in 0..icon_sz {
            let xx = x + ix;
            if xx < 0 || xx >= w {
                continue;
            }

            let a = mask[(iy * icon_sz + ix) as usize];
            if a == 0 {
                continue;
            }

            let idx = row_off + xx as usize;
            blend_over(&mut body[idx], tint, a);
        }
    }
}
