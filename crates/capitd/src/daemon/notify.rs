// Author: Dustin Pilgrim
// License: MIT
//
// Desktop notifications via org.freedesktop.Notifications
// Best-effort: failures should never break captures.

use std::collections::HashMap;

use zbus::{Connection, Proxy};
use zbus::zvariant::Value;

const DEST: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const IFACE: &str = "org.freedesktop.Notifications";

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Info,
    Error,
}

fn urgency(kind: Kind) -> u8 {
    // 0=low, 1=normal, 2=critical
    match kind {
        Kind::Info => 1,
        Kind::Error => 2,
    }
}

fn default_timeout_ms(kind: Kind) -> i32 {
    match kind {
        Kind::Info => 2500,
        Kind::Error => 6000,
    }
}

/// Send a desktop notification (best-effort).
pub fn send(kind: Kind, summary: &str, body: &str) -> Result<(), String> {
    zbus::block_on(async {
        let conn = Connection::session()
            .await
            .map_err(|e| format!("notify: dbus session connect: {e}"))?;

        let proxy = Proxy::new(&conn, DEST, PATH, IFACE)
            .await
            .map_err(|e| format!("notify: proxy: {e}"))?;

        // Notify(app_name, replaces_id, app_icon, summary, body, actions, hints, expire_timeout)
        let app_name = "Capit";
        let replaces_id: u32 = 0;
        let app_icon = ""; // optional: set an icon name later (e.g. "camera-photo")
        let actions: Vec<&str> = vec![];

        let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
        hints.insert("urgency", Value::from(urgency(kind)));

        let expire_timeout: i32 = default_timeout_ms(kind);

        let _: u32 = proxy
            .call(
                "Notify",
                &(
                    app_name,
                    replaces_id,
                    app_icon,
                    summary,
                    body,
                    actions,
                    hints,
                    expire_timeout,
                ),
            )
            .await
            .map_err(|e| format!("notify: call Notify: {e}"))?;

        Ok(())
    })
}

/// Convenience: "Saved" notification.
pub fn notify_saved(path: &std::path::Path) -> Result<(), String> {
    send(Kind::Info, "Screenshot saved", &path.display().to_string())
}

/// Convenience: "Failed" notification.
pub fn notify_failed(msg: &str) -> Result<(), String> {
    send(Kind::Error, "Screenshot failed", msg)
}
