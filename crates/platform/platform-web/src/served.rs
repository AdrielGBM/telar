//! The inputs a prerendered page carries for the app that takes it over (`telar-state`), read with the browser's own JSON parser so a page build links no parser of its own for them.

use js_sys::{Array, JSON, Object, Reflect};
use platform_core::{ColorScheme, SystemPreferences};
use wasm_bindgen::{JsCast, JsValue};

/// What a prerendered page says it was built from.
#[derive(Clone, Debug, PartialEq)]
pub struct ServedState {
    /// The format the page was written in.
    pub version: u32,
    /// The locale the app was in.
    pub locale: Option<String>,
    /// The system preferences the page assumed.
    pub preferences: SystemPreferences,
    /// The surface size the page was laid out at, in CSS pixels.
    pub surface: (u32, u32),
    /// Every keyed value the page carries, as JSON by key.
    pub signals: Vec<(String, String)>,
}

/// The state in the page's element with `id`, or `None` for a page without one or with one that cannot be read.
pub fn served_state(id: &str) -> Option<ServedState> {
    parse_served_state(&crate::dom::element_text(id)?)
}

/// The state `json` holds.
pub fn parse_served_state(json: &str) -> Option<ServedState> {
    let state = JSON::parse(json).ok()?;
    let surface = field(&state, "surface")?;
    let preferences = field(&state, "preferences");
    let preference = |name: &str| preferences.as_ref().and_then(|p| field(p, name));
    Some(ServedState {
        version: number(&field(&state, "version")?)?,
        locale: field(&state, "locale").and_then(|locale| locale.as_string()),
        preferences: SystemPreferences {
            color_scheme: match preference("color_scheme")
                .and_then(|scheme| scheme.as_string())
                .as_deref()
            {
                Some("light") => Some(ColorScheme::Light),
                Some("dark") => Some(ColorScheme::Dark),
                _ => None,
            },
            reduced_motion: preference("reduced_motion").and_then(|on| on.as_bool()),
            high_contrast: preference("high_contrast").and_then(|on| on.as_bool()),
            locales: preference("locales")
                .map(|locales| {
                    Array::from(&locales)
                        .iter()
                        .filter_map(|locale| locale.as_string())
                        .collect()
                })
                .unwrap_or_default(),
        },
        surface: (
            number(&field(&surface, "width")?)?,
            number(&field(&surface, "height")?)?,
        ),
        signals: field(&state, "signals")
            .and_then(|signals| signals.dyn_into::<Object>().ok())
            .map(|signals| {
                Object::entries(&signals)
                    .iter()
                    .filter_map(|entry| {
                        let entry = Array::from(&entry);
                        Some((entry.get(0).as_string()?, entry.get(1).as_string()?))
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn field(object: &JsValue, name: &str) -> Option<JsValue> {
    Reflect::get(object, &JsValue::from_str(name))
        .ok()
        .filter(|value| !value.is_undefined() && !value.is_null())
}

fn number(value: &JsValue) -> Option<u32> {
    value
        .as_f64()
        .filter(|n| n.fract() == 0.0 && *n >= 0.0 && *n <= u32::MAX as f64)
        .map(|n| n as u32)
}

#[cfg(test)]
#[path = "served_test.rs"]
mod tests;
