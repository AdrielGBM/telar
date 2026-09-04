//! Linux OS light/dark detection via the freedesktop settings portal. winit 0.30 has no color-scheme integration on Linux (`Window::theme()` is always `None` there), so the desktop adapter reads the portal directly — a one-shot at window creation plus a background watch for live changes. Windows/macOS get the preference straight from winit and never reach this module.

/// Reads `org.freedesktop.appearance` `color-scheme` (1 = prefer dark, 2 = prefer light, 0 = no preference). `None` when the portal is absent or reports no preference, so the app keeps its own default. Blocking.
pub fn portal_prefers_dark() -> Option<bool> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let reply = conn
        .call_method(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            Some("org.freedesktop.portal.Settings"),
            "Read",
            &("org.freedesktop.appearance", "color-scheme"),
        )
        .ok()?;
    let value: zbus::zvariant::OwnedValue = reply.body().deserialize().ok()?;
    match scheme_u32(&value) {
        Some(1) => Some(true),
        Some(2) => Some(false),
        _ => None,
    }
}

// The portal double-wraps the value in variants (`v` → `v` → `u`); unwrap nested variants to the integer.
fn scheme_u32(v: &zbus::zvariant::Value) -> Option<u32> {
    use zbus::zvariant::Value;
    match v {
        Value::U32(n) => Some(*n),
        Value::U8(n) => Some(*n as u32),
        Value::I32(n) => Some(*n as u32),
        Value::Value(inner) => scheme_u32(inner),
        _ => None,
    }
}

/// Watches the portal's `SettingChanged` signal on a background thread and calls `on_change(dark)` whenever the OS color-scheme flips, so the app reacts live. The thread lives for the process; silently returns if the session bus is unavailable.
pub fn spawn_watch(on_change: impl Fn(bool) + Send + 'static) {
    std::thread::Builder::new()
        .name("telar-color-scheme".to_string())
        .spawn(move || {
            let Ok(conn) = zbus::blocking::Connection::session() else {
                return;
            };
            let Ok(proxy) = zbus::blocking::Proxy::new(
                &conn,
                "org.freedesktop.portal.Desktop",
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.Settings",
            ) else {
                return;
            };
            let Ok(signals) = proxy.receive_signal("SettingChanged") else {
                return;
            };
            for msg in signals {
                let Ok((namespace, key, value)) =
                    msg.body()
                        .deserialize::<(String, String, zbus::zvariant::OwnedValue)>()
                else {
                    continue;
                };
                if namespace == "org.freedesktop.appearance" && key == "color-scheme" {
                    if let Some(n) = scheme_u32(&value) {
                        on_change(n == 1);
                    }
                }
            }
        })
        .ok();
}
