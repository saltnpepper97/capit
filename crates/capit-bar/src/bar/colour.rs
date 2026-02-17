// colour.rs
// Author: Dustin Pilgrim
// License: MIT
//
// Small ARGB helpers for deriving UI colours from a base background.
// Format: 0xAARRGGBB

#[inline]
pub(crate) fn a(argb: u32) -> u8 { ((argb >> 24) & 0xFF) as u8 }
#[inline]
pub(crate) fn r(argb: u32) -> u8 { ((argb >> 16) & 0xFF) as u8 }
#[inline]
pub(crate) fn g(argb: u32) -> u8 { ((argb >> 8) & 0xFF) as u8 }
#[inline]
pub(crate) fn b(argb: u32) -> u8 { (argb & 0xFF) as u8 }

#[inline]
pub(crate) fn argb(a: u8, r: u8, g: u8, b: u8) -> u32 {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

#[inline]
fn clamp_u8(v: i32) -> u8 {
    if v < 0 { 0 } else if v > 255 { 255 } else { v as u8 }
}

/// Additive lighten/darken by signed delta per channel.
/// Keeps alpha unchanged.
#[inline]
pub(crate) fn shift_rgb(base: u32, delta: i16) -> u32 {
    let aa = a(base);
    let rr = clamp_u8(r(base) as i32 + delta as i32);
    let gg = clamp_u8(g(base) as i32 + delta as i32);
    let bb = clamp_u8(b(base) as i32 + delta as i32);
    argb(aa, rr, gg, bb)
}

/// Blend `base` towards `target` by `t` in [0.0, 1.0]. Keeps alpha from base.
#[inline]
pub(crate) fn mix_rgb(base: u32, target: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let aa = a(base);

    let br = r(base) as f32;
    let bg = g(base) as f32;
    let bb = b(base) as f32;

    let tr = r(target) as f32;
    let tg = g(target) as f32;
    let tb = b(target) as f32;

    let rr = (br + (tr - br) * t).round() as i32;
    let gg = (bg + (tg - bg) * t).round() as i32;
    let bb2 = (bb + (tb - bb) * t).round() as i32;

    argb(aa, clamp_u8(rr), clamp_u8(gg), clamp_u8(bb2))
}

/// Slightly lighten a base colour. `amount` is 0..=255-ish (small numbers recommended).
#[inline]
pub(crate) fn lighten(base: u32, amount: u8) -> u32 {
    shift_rgb(base, amount as i16)
}

/// Slightly darken a base colour. `amount` is 0..=255-ish (small numbers recommended).
#[inline]
pub(crate) fn darken(base: u32, amount: u8) -> u32 {
    shift_rgb(base, -(amount as i16))
}

/// Make a “glow” colour from `accent` but with a new alpha.
/// Keeps RGB from accent, swaps alpha.
#[inline]
pub(crate) fn with_alpha(accent: u32, alpha: u8) -> u32 {
    (accent & 0x00FF_FFFF) | ((alpha as u32) << 24)
}

/// A nice default set of derived slot colours from a bar background.
/// Tuned for small deltas so it works across lots of themes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SlotColours {
    pub idle: u32,
    pub hover: u32,
    pub selected: u32,
    pub disabled: u32,
    pub disabled_border: u32,
    pub border: u32,
}

pub(crate) fn derive_slot_colours(bar_bg: u32) -> SlotColours {
    // These are intentionally subtle. You can tweak in one place later.
    let border = darken(bar_bg, 18);
    SlotColours {
        idle:     lighten(bar_bg, 8),
        hover:    lighten(bar_bg, 16),
        selected: lighten(bar_bg, 26),
        disabled: darken(bar_bg, 6),
        disabled_border: darken(bar_bg, 14),
        border,
    }
}
