// Author: Dustin Pilgrim
// License: MIT
//
// Screenshot capture via xdg-desktop-portal (org.freedesktop.portal.Desktop)
// Uses async zbus internally but exposes blocking functions.
// Waits for the Request::Response signal and copies the resulting
// file:// URI to the requested output path.
//
// The portal Screenshot() method returns a screenshot of the *entire*
// desktop. To support output/region/window flows, we capture full and then crop.
//
// Notes on performance:
// - The portal call itself is often the dominant cost (backend/compositor work).
// - We avoid extra disk copies: for crop we read portal file directly and write output.
// - We avoid polling: use a single timeout select.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_io::Timer;
use futures_util::{future::select, future::Either, StreamExt};
use image::GenericImageView;
use zbus::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use capit_core::Rect;

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const SCREENSHOT_IFACE: &str = "org.freedesktop.portal.Screenshot";
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

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
/// Notes:
/// - Requires xdg-desktop-portal + a backend (gtk/kde/wlr/etc).
/// - May show a permission dialog depending on portal config.
pub fn capture_screen_to(out_path: &Path) -> Result<(), String> {
    ensure_parent_dir(out_path)?;

    // Portal returns a file path; copy it directly to the requested destination.
    let src_path = capture_portal_screenshot_path()?;
    fs::copy(&src_path, out_path).map_err(|e| {
        format!(
            "copy portal screenshot {src_path:?} -> {out_path:?}: {e}"
        )
    })?;

    Ok(())
}

/// Capture a screenshot, then crop and save to `out_path`.
pub fn capture_screen_to_crop(out_path: &Path, crop: CaptureCrop) -> Result<(), String> {
    ensure_parent_dir(out_path)?;

    // Avoid extra copy: read/crop directly from portal-produced file.
    let src_path = capture_portal_screenshot_path()?;
    save_cropped_png(&src_path, out_path, crop)
}

/// Capture a screenshot, then crop using a `capit_core::Rect`.
pub fn capture_screen_to_rect(out_path: &Path, rect: &Rect) -> Result<(), String> {
    capture_screen_to_crop(out_path, CaptureCrop::from_rect(rect))
}

/// Internal: call portal Screenshot() and return the portal-created file path.
///
/// We rely on the portal file existing long enough to read/copy it immediately.
/// (In practice this is fine — the portal backend creates it for the request result.)
fn capture_portal_screenshot_path() -> Result<PathBuf, String> {
    zbus::block_on(async {
        let conn = Connection::session()
            .await
            .map_err(|e| format!("dbus session connect: {e}"))?;

        let screenshot = Proxy::new(&conn, PORTAL_DEST, PORTAL_PATH, SCREENSHOT_IFACE)
            .await
            .map_err(|e| format!("proxy screenshot: {e}"))?;

        let token = new_handle_token();
        let mut options: HashMap<&str, Value<'_>> = HashMap::new();
        options.insert("handle_token", Value::from(token.as_str()));
        options.insert("interactive", Value::from(false));

        // Optional parent window handle (we're headless)
        let parent_window = "";

        let request_path: OwnedObjectPath = screenshot
            .call("Screenshot", &(parent_window, options))
            .await
            .map_err(|e| format!("portal Screenshot() call failed: {e}"))?;

        let request = Proxy::new(&conn, PORTAL_DEST, request_path.clone(), REQUEST_IFACE)
            .await
            .map_err(|e| format!("proxy request: {e}"))?;

        let mut stream = request
            .receive_signal("Response")
            .await
            .map_err(|e| format!("receive Response signal: {e}"))?;

        // Wait for either the response signal or a hard timeout.
        let next = stream.next();
        let timeout = Timer::after(Duration::from_secs(30));

        match select(next, timeout).await {
            Either::Left((Some(msg), _)) => {
                let (response, results): (u32, HashMap<String, OwnedValue>) = msg
                    .body()
                    .deserialize()
                    .map_err(|e| format!("signal decode: {e}"))?;

                if response != 0 {
                    return Err(format!("portal screenshot failed (response={response})"));
                }

                let uri: &str = results
                    .get("uri")
                    .ok_or_else(|| "portal response missing 'uri'".to_string())?
                    .downcast_ref::<&str>()
                    .map_err(|e| format!("'uri' had unexpected type: {e}"))?;

                uri_to_path(uri)
            }
            Either::Left((None, _)) => Err("portal signal stream ended unexpectedly".into()),
            Either::Right((_, _)) => Err("portal request timed out waiting for Response".into()),
        }
    })
}

fn save_cropped_png(src_path: &Path, out_path: &Path, crop: CaptureCrop) -> Result<(), String> {
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

fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create dir {parent:?}: {e}"))?;
    }
    Ok(())
}

fn uri_to_path(uri: &str) -> Result<PathBuf, String> {
    const PREFIX: &str = "file://";

    if !uri.starts_with(PREFIX) {
        return Err(format!("unexpected uri scheme: {uri}"));
    }

    // file:///home/... → /home/...
    let mut p = &uri[PREFIX.len()..];
    while p.starts_with("//") {
        p = &p[1..];
    }

    Ok(PathBuf::from(p))
}

fn new_handle_token() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("capit_{now}")
}
