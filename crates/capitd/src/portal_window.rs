// Author: Dustin Pilgrim
// License: MIT
//
// Window selection via xdg-desktop-portal ScreenCast (WINDOW-only).
//
// Flow (per org.freedesktop.portal.ScreenCast spec):
// 1) ScreenCast.CreateSession -> returns Request handle (o)
//    Response results contain: session_handle (o)
// 2) ScreenCast.SelectSources(session_handle, opts) -> returns Request handle (o)
// 3) ScreenCast.Start(session_handle, parent_window, opts) -> returns Request handle (o)
//    Response results contain: streams a(ua{sv}) (node_id is in properties)
// 4) ScreenCast.OpenPipeWireRemote(session_handle, opts) -> returns fd (h)
//
// NOTE:
// - This only performs portal selection and returns (session_handle, pipewire_fd, node_id).
// - Reading frames from PipeWire is a separate step.

use std::collections::HashMap;
use std::convert::TryInto;
use std::os::fd::OwnedFd;

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

#[derive(Debug)]
pub struct WindowPortalSelection {
    pub session: OwnedObjectPath,
    pub pipewire_fd: OwnedFd,
    pub node_id: u32,
}

pub fn select_window_pipewire_stream() -> Result<WindowPortalSelection, String> {
    let conn = Connection::session().map_err(|e| format!("portal: connect session bus: {e}"))?;

    let sc = Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.ScreenCast",
    )
    .map_err(|e| format!("portal: create ScreenCast proxy: {e}"))?;

    ensure_window_sources_supported(&sc)?;

    // 1) CreateSession (returns Request handle; session_handle comes in Response results)
    let token_create_req = fresh_token("capit_create_req");
    let token_session = fresh_token("capit_session");

    let mut create_opts: HashMap<&str, Value> = HashMap::new();
    create_opts.insert("handle_token", token_create_req.as_str().into());
    create_opts.insert("session_handle_token", token_session.as_str().into());

    let create_req: OwnedObjectPath = sc
        .call("CreateSession", &(create_opts))
        .map_err(|e| format!("portal: CreateSession call failed: {e}"))?;

    let create_results = wait_request_results(&conn, create_req.as_ref())?;
    let session_handle = parse_session_handle(&create_results)
        .ok_or_else(|| "portal: CreateSession returned no session_handle".to_string())?;

    // 2) SelectSources (WINDOW only)
    // types bitmask: 1=MONITOR, 2=WINDOW, 4=VIRTUAL
    let token_select_req = fresh_token("capit_select_req");

    let mut select_opts: HashMap<&str, Value> = HashMap::new();
    select_opts.insert("handle_token", token_select_req.as_str().into());
    select_opts.insert("types", (2u32).into()); // WINDOW only
    select_opts.insert("multiple", false.into());

    let select_req: OwnedObjectPath = sc
        .call("SelectSources", &(session_handle.as_ref(), select_opts))
        .map_err(|e| format!("portal: SelectSources call failed: {e}"))?;

    wait_request_ok(&conn, select_req.as_ref())?;

    // 3) Start
    let token_start_req = fresh_token("capit_start_req");
    let mut start_opts: HashMap<&str, Value> = HashMap::new();
    start_opts.insert("handle_token", token_start_req.as_str().into());

    // parent_window: empty string for CLI apps
    let start_req: OwnedObjectPath = sc
        .call("Start", &(session_handle.as_ref(), "", start_opts))
        .map_err(|e| format!("portal: Start call failed: {e}"))?;

    let start_results = wait_request_results(&conn, start_req.as_ref())?;
    let node_id = parse_first_node_id(&start_results)
        .ok_or_else(|| "portal: Start returned no window stream node_id".to_string())?;

    // 4) OpenPipeWireRemote
    let open_opts: HashMap<&str, Value> = HashMap::new();
    let fd: zbus::zvariant::OwnedFd = sc
        .call("OpenPipeWireRemote", &(session_handle.as_ref(), open_opts))
        .map_err(|e| format!("portal: OpenPipeWireRemote failed: {e}"))?;

    let pipewire_fd: OwnedFd = fd.into();

    Ok(WindowPortalSelection {
        session: session_handle,
        pipewire_fd,
        node_id,
    })
}

fn fresh_token(prefix: &str) -> String {
    // Must be a valid object path element: stick to [A-Za-z0-9_]
    let mut p = String::with_capacity(prefix.len());
    for ch in prefix.chars() {
        if ch.is_ascii_alphanumeric() {
            p.push(ch);
        } else {
            p.push('_');
        }
    }

    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    format!("{p}_{pid}_{nanos}")
}

fn wait_request_ok(conn: &Connection, request_path: ObjectPath<'_>) -> Result<(), String> {
    let (code, _results) = wait_request(conn, request_path)?;
    match code {
        0 => Ok(()),
        1 => Err("cancelled".into()),
        other => Err(format!("portal request failed (code={other})")),
    }
}

fn wait_request_results(
    conn: &Connection,
    request_path: ObjectPath<'_>,
) -> Result<HashMap<String, OwnedValue>, String> {
    let (code, results) = wait_request(conn, request_path)?;
    match code {
        0 => Ok(results),
        1 => Err("cancelled".into()),
        other => Err(format!("portal request failed (code={other})")),
    }
}

/// Listen for org.freedesktop.portal.Request::Response(u32, a{sv}) on the request object path.
fn wait_request(
    conn: &Connection,
    request_path: ObjectPath<'_>,
) -> Result<(u32, HashMap<String, OwnedValue>), String> {
    let req = Proxy::new(
        conn,
        "org.freedesktop.portal.Desktop",
        request_path.as_str(),
        "org.freedesktop.portal.Request",
    )
    .map_err(|e| format!("portal: create Request proxy: {e}"))?;

    let mut stream = req
        .receive_signal("Response")
        .map_err(|e| format!("portal: receive Response signal: {e}"))?;

    let msg = stream
        .next()
        .ok_or_else(|| "portal: request signal stream ended unexpectedly".to_string())?;

    let (code, results): (u32, HashMap<String, OwnedValue>) = msg
        .body()
        .deserialize()
        .map_err(|e| format!("portal: decode Response body: {e}"))?;

    Ok((code, results))
}

fn parse_session_handle(results: &HashMap<String, OwnedValue>) -> Option<OwnedObjectPath> {
    let v = results.get("session_handle")?;
    v.clone().try_into().ok()
}

fn parse_first_node_id(results: &HashMap<String, OwnedValue>) -> Option<u32> {
    let streams_val = results.get("streams")?;
    let streams: Vec<(u32, HashMap<String, OwnedValue>)> = streams_val.clone().try_into().ok()?;

    let (_stream_id, props) = streams.first()?;

    // node_id:u32 (common)
    if let Some(n) = props.get("node_id") {
        if let Ok(u) = TryInto::<u32>::try_into(n.clone()) {
            return Some(u);
        }
        if let Ok(u) = TryInto::<u64>::try_into(n.clone()) {
            return Some(u as u32);
        }
    }

    // Some backends may use alternate keys
    for k in ["pipewire_node", "node"] {
        if let Some(n) = props.get(k) {
            if let Ok(u) = TryInto::<u32>::try_into(n.clone()) {
                return Some(u);
            }
            if let Ok(u) = TryInto::<u64>::try_into(n.clone()) {
                return Some(u as u32);
            }
        }
    }

    None
}

fn ensure_window_sources_supported(sc: &Proxy<'_>) -> Result<(), String> {
    // Bitmask: 1=MONITOR, 2=WINDOW, 4=VIRTUAL
    let available: u32 = sc
        .get_property("AvailableSourceTypes")
        .map_err(|e| format!("portal: read AvailableSourceTypes: {e}"))?;

    if (available & 2) != 0 {
        return Ok(());
    }

    let xdg = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    if xdg.to_ascii_lowercase().contains("labwc") {
        return Err(
            "window capture is not available on labwc via xdg-desktop-portal right now \
(AvailableSourceTypes reports MONITOR-only). Use screen/region capture instead."
                .into(),
        );
    }

    Err(format!(
        "window capture is not supported by the current portal backend/compositor \
(AvailableSourceTypes={available}, WINDOW bit missing)."
    ))
}
