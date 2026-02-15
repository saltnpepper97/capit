// Author: Dustin Pilgrim
// License: MIT
//
// TEMPORARY CAPTURE IMPLEMENTATION
// -------------------------------
// Portal/zbus capture has been removed for now.
// Screen/region capture will be re-implemented per compositor backend (sway/niri/hyprland).
//
// The API remains the same so the rest of capitd can compile and you can keep building UI/IPC.
// For now, capture_* functions return a clear error unless you wire in a backend.

use std::fs;
use std::path::{Path, PathBuf};

use image::GenericImageView;

use capit_core::Rect;

#[derive(Debug, Clone, Copy)]
pub struct CaptureCrop {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl CaptureCrop {
    pub fn from_rect(r: &Rect) -> Self {
        Self {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
        }
    }
}

/// Capture a full screenshot and write it to `out_path`.
///
/// TEMPORARY: not implemented (portal removed; compositor backends pending).
pub fn capture_screen_to(_out_path: &Path) -> Result<(), String> {
    Err("capture_screen_to: not implemented (portal removed; compositor backends pending)"
        .to_string())
}

/// Capture a screenshot, then crop and save to `out_path`.
///
/// TEMPORARY: not implemented until a backend provides a real screenshot source.
pub fn capture_screen_to_crop(_out_path: &Path, _crop: CaptureCrop) -> Result<(), String> {
    Err("capture_screen_to_crop: not implemented (portal removed; compositor backends pending)"
        .to_string())
}

/// Capture a screenshot, then crop using a `capit_core::Rect`.
pub fn capture_screen_to_rect(out_path: &Path, rect: &Rect) -> Result<(), String> {
    capture_screen_to_crop(out_path, CaptureCrop::from_rect(rect))
}

// --- Helpers you can reuse once you have a screenshot source file/path ---

/// Given an existing screenshot file `src_path`, crop and save to `out_path`.
/// Keep this: it will be useful once your backend produces a PNG path.
pub fn save_cropped_png(src_path: &Path, out_path: &Path, crop: CaptureCrop) -> Result<(), String> {
    ensure_parent_dir(out_path)?;

    let img = image::open(src_path).map_err(|e| format!("open screenshot: {e}"))?;
    let (iw, ih) = img.dimensions();

    let x = crop.x;
    let y = crop.y;
    let w = crop.w;
    let h = crop.h;

    // Clamp to image bounds (avoid panics)
    let x0 = x.max(0) as u32;
    let y0 = y.max(0) as u32;

    let x1 = (x.max(0) as u32)
        .saturating_add(w.max(0) as u32)
        .min(iw);
    let y1 = (y.max(0) as u32)
        .saturating_add(h.max(0) as u32)
        .min(ih);

    let cw = x1.saturating_sub(x0);
    let ch = y1.saturating_sub(y0);

    if cw == 0 || ch == 0 {
        return Err(format!(
            "crop rect empty after clamping: ({x},{y}) {w}x{h} within {iw}x{ih}"
        ));
    }

    let cropped = img.crop_imm(x0, y0, cw, ch);
    cropped
        .save(out_path)
        .map_err(|e| format!("save cropped screenshot: {e}"))?;

    Ok(())
}

/// Copy an existing screenshot file directly to `out_path`.
/// Useful once a backend returns a temp file path.
pub fn copy_screenshot(src_path: &Path, out_path: &Path) -> Result<(), String> {
    ensure_parent_dir(out_path)?;
    fs::copy(src_path, out_path)
        .map_err(|e| format!("copy screenshot {src_path:?} -> {out_path:?}: {e}"))?;
    Ok(())
}

fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create dir {parent:?}: {e}"))?;
    }
    Ok(())
}

/// Placeholder for future backend: return a path to a screenshot file.
/// Kept as a stub so you can wire it later without reshuffling callsites.
#[allow(dead_code)]
fn capture_backend_screenshot_path() -> Result<PathBuf, String> {
    Err("capture backend not implemented yet".to_string())
}
