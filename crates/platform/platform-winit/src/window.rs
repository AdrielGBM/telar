use std::sync::Arc;

use platform_core::Window as PlatformWindow;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use winit::window::Window as WinitInnerWindow;

#[derive(Clone)]
pub struct WinitWindow(pub Arc<WinitInnerWindow>);

impl HasWindowHandle for WinitWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for WinitWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

impl PlatformWindow for WinitWindow {
    fn width(&self) -> u32 {
        self.0.inner_size().width
    }

    fn height(&self) -> u32 {
        self.0.inner_size().height
    }

    fn request_redraw(&self) {
        self.0.request_redraw();
    }

    fn scale_factor(&self) -> f64 {
        self.0.scale_factor()
    }

    fn prefers_dark(&self) -> Option<bool> {
        if let Some(t) = self.0.theme() {
            return Some(t == winit::window::Theme::Dark);
        }
        // winit 0.30 has no color-scheme integration on Linux (theme() is always None there — verified in its
        // source), so fall back to the freedesktop settings portal, which KDE/GNOME/wlroots desktops expose.
        #[cfg(target_os = "linux")]
        {
            linux_portal_prefers_dark()
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }
}

// Reads `org.freedesktop.appearance` `color-scheme` from the xdg-desktop-portal (1 = prefer dark, 2 = prefer
// light, 0 = no preference). Returns `None` when the portal is absent or reports no preference, so the app
// keeps its own default. A blocking one-shot at window creation; live changes are a separate follow-up.
#[cfg(target_os = "linux")]
fn linux_portal_prefers_dark() -> Option<bool> {
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
    match portal_scheme_u32(&value) {
        Some(1) => Some(true),
        Some(2) => Some(false),
        _ => None,
    }
}

// The portal double-wraps the value in variants (`v` → `v` → `u`); unwrap nested variants to the integer.
#[cfg(target_os = "linux")]
fn portal_scheme_u32(v: &zbus::zvariant::Value) -> Option<u32> {
    use zbus::zvariant::Value;
    match v {
        Value::U32(n) => Some(*n),
        Value::U8(n) => Some(*n as u32),
        Value::I32(n) => Some(*n as u32),
        Value::Value(inner) => portal_scheme_u32(inner),
        _ => None,
    }
}

/// Watches the freedesktop portal's `SettingChanged` signal on a background thread and calls `on_change(dark)`
/// whenever the OS color-scheme flips, so an app can react live. Linux-only (winit reports theme changes
/// natively elsewhere); the thread lives for the process. Silently returns if the session bus is unavailable.
#[cfg(target_os = "linux")]
pub fn spawn_color_scheme_watch(on_change: impl Fn(bool) + Send + 'static) {
    std::thread::Builder::new()
        .name("rsx-color-scheme".to_string())
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
                    if let Some(n) = portal_scheme_u32(&value) {
                        on_change(n == 1);
                    }
                }
            }
        })
        .ok();
}
