// Author: Dustin Pilgrim
// License: MIT
//
// Screenshot capture via xdg-desktop-portal (org.freedesktop.portal.Desktop)
// Uses async zbus internally but exposes blocking functions.
// Waits for the Request::Response signal and copies the resulting
// file:// URI to the requested output path.
//
// The portal Screenshot() method (as used here) returns a screenshot of the *entire*
// desktop. To support `--output` and region/window flows, we capture full and then crop.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_io::Timer;
use futures_util::{future::select, future::Either, pin_mut, StreamExt};

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
    /// Convert a core Rect to a crop rect.
    /// Adjust these field names if your Rect uses width/height naming.
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

    let src_path = capture_portal_to_temp_file(out_path)?;
    fs::copy(&src_path, out_path)
        .map_err(|e| format!("copy {src_path:?} -> {out_path:?}: {e}"))?;
    let _ = fs::remove_file(&src_path);
    Ok(())
}

/// Capture a screenshot, then crop and save to `out_path`.
///
/// This is used for `--output`, and for region/window once you have rects.
pub fn capture_screen_to_crop(out_path: &Path, crop: CaptureCrop) -> Result<(), String> {
    ensure_parent_dir(out_path)?;

    let src_path = capture_portal_to_temp_file(out_path)?;
    let res = save_cropped_png(&src_path, out_path, crop);
    let _ = fs::remove_file(&src_path);
    res
}

/// Capture a screenshot, then crop using a `capit_core::Rect`.
///
/// Intended for Region selection (once your UI produces a rect).
pub fn capture_screen_to_rect(out_path: &Path, rect: &Rect) -> Result<(), String> {
    capture_screen_to_crop(out_path, CaptureCrop::from_rect(rect))
}

/// Internal: call portal Screenshot() and return a temp PNG path on disk.
///
/// We always capture “full desktop” here; selection happens via cropping.
fn capture_portal_to_temp_file(final_out_path: &Path) -> Result<PathBuf, String> {
    zbus::block_on(async {
        ensure_parent_dir(final_out_path)?;

        let conn = Connection::session().await.map_err(|e| {
            // This is the root cause of your:
            //   dbus session connect: I/O error: No such file or directory (os error 2)
            // Most commonly: missing XDG_RUNTIME_DIR and/or DBUS_SESSION_BUS_ADDRESS
            let xdg = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "(unset)".into());
            let addr =
                std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_else(|_| "(unset)".into());
            format!(
                "dbus session connect: {e} (XDG_RUNTIME_DIR={xdg}, DBUS_SESSION_BUS_ADDRESS={addr})"
            )
        })?;

        let screenshot = Proxy::new(&conn, PORTAL_DEST, PORTAL_PATH, SCREENSHOT_IFACE)
            .await
            .map_err(|e| format!("proxy screenshot: {e}"))?;

        let token = new_handle_token();
        let mut options: HashMap<&str, Value<'_>> = HashMap::new();
        options.insert("handle_token", Value::from(token.as_str()));
        options.insert("interactive", Value::from(false));

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

        let deadline = std::time::Instant::now() + Duration::from_secs(30);

        loop {
            if std::time::Instant::now() > deadline {
                return Err("portal request timed out waiting for Response".into());
            }

            let next_signal = stream.next();
            let timeout = Timer::after(Duration::from_millis(250));
            pin_mut!(next_signal, timeout);

            let msg = match select(next_signal, timeout).await {
                Either::Left((Some(msg), _)) => msg,
                Either::Left((None, _)) => {
                    return Err("portal signal stream ended unexpectedly".into())
                }
                Either::Right((_, _)) => continue,
            };

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

            let src_path = uri_to_path(uri)?;

            // Copy portal-produced file into a stable temp path next to the final output.
            // (The portal temp file may get cleaned up; we want our own.)
            let tmp_out = temp_output_path(final_out_path);
            fs::copy(&src_path, &tmp_out)
                .map_err(|e| format!("copy {src_path:?} -> {tmp_out:?}: {e}"))?;

            return Ok(tmp_out);
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

fn temp_output_path(final_out_path: &Path) -> PathBuf {
    // Keep extension as png, but ensure uniqueness-ish.
    // Example: shot.png -> shot.capit_tmp_<nanos>.png
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let mut p = final_out_path.to_path_buf();
    let stem = final_out_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("capit");

    p.set_file_name(format!("{stem}.capit_tmp_{nanos}.png"));
    p
}

fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create dir {parent:?}: {e}"))?;
    }
    Ok(())
}

fn uri_to_path(uri: &str) -> Result<PathBuf, String> {
    // Common forms seen in portal responses:
    //   file:///home/user/foo.png
    //   file://localhost/home/user/foo.png
    // and the path may be percent-encoded (spaces, etc.)
    const PREFIX: &str = "file://";

    if !uri.starts_with(PREFIX) {
        return Err(format!("unexpected uri scheme: {uri}"));
    }

    let mut p = &uri[PREFIX.len()..];

    // Drop optional authority. If present, it must be empty or "localhost".
    if let Some(slash) = p.find('/') {
        let (authority, rest) = p.split_at(slash);
        if !authority.is_empty() && authority != "localhost" {
            return Err(format!("unsupported file uri authority '{authority}' in {uri}"));
        }
        p = rest;
    } else {
        return Err(format!("malformed file uri (no path): {uri}"));
    }

    // file:////home/... is odd but occasionally appears; normalize leading '//' -> '/'
    while p.starts_with("//") {
        p = &p[1..];
    }

    let decoded = percent_decode(p)?;
    Ok(PathBuf::from(decoded))
}

fn percent_decode(s: &str) -> Result<String, String> {
    // Minimal %XX decoder for file:// URIs.
    // We do NOT treat '+' as space; file URIs should use %20 for spaces.
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(format!("bad percent-escape in path: '{s}'"));
            }
            let hi = from_hex(bytes[i + 1]).ok_or_else(|| format!("bad percent-escape in path: '{s}'"))?;
            let lo = from_hex(bytes[i + 2]).ok_or_else(|| format!("bad percent-escape in path: '{s}'"))?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }

    String::from_utf8(out).map_err(|e| format!("decoded uri path was not utf-8: {e}"))
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(10 + (b - b'a')),
        b'A'..=b'F' => Some(10 + (b - b'A')),
        _ => None,
    }
}

fn new_handle_token() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("capit_{now}")
}
