//! The freedesktop settings portal over D-Bus: one read of the appearance namespace, and a watch for its change signal.

use std::collections::HashMap;

use platform_core::SystemPreferences;
use zbus::zvariant::{OwnedValue, Value};

use super::appearance::{AppearanceSetting, from_settings};

const NAMESPACE: &str = "org.freedesktop.appearance";
const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.Settings";

/// The appearance keys the portal answers, with every field unknown where there is no session bus, no portal, or a portal too old to carry that key. Blocking.
pub fn read() -> SystemPreferences {
    read_all().unwrap_or_default()
}

fn read_all() -> Option<SystemPreferences> {
    let connection = zbus::blocking::Connection::session().ok()?;
    let reply = connection
        .call_method(
            Some(DESTINATION),
            PATH,
            Some(INTERFACE),
            "ReadAll",
            &(vec![NAMESPACE],),
        )
        .ok()?;
    let namespaces: HashMap<String, HashMap<String, OwnedValue>> =
        reply.body().deserialize().ok()?;
    let settings = namespaces.get(NAMESPACE)?;
    Some(from_settings(settings.iter().filter_map(|(key, value)| {
        Some((key.as_str(), as_u32(value)?))
    })))
}

/// Calls `on_change` from a background thread for every appearance key the portal reports changed. The thread lives for the process, and gives up quietly where there is no session bus to listen on.
pub fn spawn_watch(on_change: impl Fn(AppearanceSetting) + Send + 'static) {
    std::thread::Builder::new()
        .name("telar-system-preferences".to_string())
        .spawn(move || {
            let Ok(connection) = zbus::blocking::Connection::session() else {
                return;
            };
            let Ok(proxy) = zbus::blocking::Proxy::new(&connection, DESTINATION, PATH, INTERFACE)
            else {
                return;
            };
            let Ok(signals) = proxy.receive_signal("SettingChanged") else {
                return;
            };
            for message in signals {
                let Ok((namespace, key, value)) =
                    message.body().deserialize::<(String, String, OwnedValue)>()
                else {
                    continue;
                };
                if namespace != NAMESPACE {
                    continue;
                }
                if let Some(setting) =
                    as_u32(&value).and_then(|value| AppearanceSetting::parse(&key, value))
                {
                    on_change(setting);
                }
            }
        })
        .ok();
}

// Older portals answer `Read` with the value wrapped in a second variant, and a few send signed integers, so both are accepted.
fn as_u32(value: &Value) -> Option<u32> {
    match value {
        Value::U32(n) => Some(*n),
        Value::U8(n) => Some(u32::from(*n)),
        Value::I32(n) => u32::try_from(*n).ok(),
        Value::Value(inner) => as_u32(inner),
        _ => None,
    }
}
