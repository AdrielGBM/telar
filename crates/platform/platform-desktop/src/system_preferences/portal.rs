//! The freedesktop settings portal over D-Bus: one read of the appearance namespace and of the interface namespace that carries the text scale, and a watch for their change signal.

use std::collections::HashMap;

use platform_core::SystemPreferences;
use zbus::zvariant::{OwnedValue, Value};

use super::appearance::{AppearanceSetting, from_settings};

/// The namespace of the colour scheme, contrast and motion keys.
const APPEARANCE_NAMESPACE: &str = "org.freedesktop.appearance";

/// The namespace GNOME's portal publishes its interface settings under, the text scale among them; another desktop's portal leaves it out.
const INTERFACE_NAMESPACE: &str = "org.gnome.desktop.interface";

/// The key of the text scale in [`INTERFACE_NAMESPACE`].
const TEXT_SCALING_FACTOR: &str = "text-scaling-factor";

const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.Settings";

/// The appearance keys the portal answers, with every field unknown where there is no session bus, no portal, or a portal too old to carry that key, and text at its designed size where the portal publishes no text scale. Blocking.
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
            &(vec![APPEARANCE_NAMESPACE, INTERFACE_NAMESPACE],),
        )
        .ok()?;
    let namespaces: HashMap<String, HashMap<String, OwnedValue>> =
        reply.body().deserialize().ok()?;
    let mut preferences = from_settings(
        namespaces
            .get(APPEARANCE_NAMESPACE)
            .into_iter()
            .flatten()
            .filter_map(|(key, value)| Some((key.as_str(), as_u32(value)?))),
    );
    if let Some(factor) = namespaces
        .get(INTERFACE_NAMESPACE)
        .and_then(|settings| settings.get(TEXT_SCALING_FACTOR))
        .and_then(|value| as_f64(value))
    {
        AppearanceSetting::text_scaling_factor(factor).apply(&mut preferences);
    }
    Some(preferences)
}

/// The setting a `SettingChanged` signal reports, or `None` for one the preferences do not carry.
fn changed_setting(namespace: &str, key: &str, value: &Value) -> Option<AppearanceSetting> {
    match namespace {
        APPEARANCE_NAMESPACE => AppearanceSetting::parse(key, as_u32(value)?),
        INTERFACE_NAMESPACE if key == TEXT_SCALING_FACTOR => {
            Some(AppearanceSetting::text_scaling_factor(as_f64(value)?))
        }
        _ => None,
    }
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
                if let Some(setting) = changed_setting(&namespace, &key, &value) {
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

fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::F64(n) => Some(*n),
        Value::Value(inner) => as_f64(inner),
        _ => None,
    }
}

#[cfg(test)]
#[path = "portal.test.rs"]
mod tests;
