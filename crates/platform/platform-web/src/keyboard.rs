//! Which keyboard the person at the browser has, read from what the browser says it runs on.

use platform_core::KeyboardConvention;

use crate::dom;

/// The convention of the system the browser runs on: `navigator.userAgentData.platform` where the browser offers it, else `navigator.platform`, and a PC keyboard when it says neither.
pub fn keyboard_convention() -> KeyboardConvention {
    KeyboardConvention::of_platform_name(&platform_name().unwrap_or_default())
}

fn platform_name() -> Option<String> {
    let navigator = dom::window().navigator();
    js_sys::Reflect::get(&navigator, &"userAgentData".into())
        .ok()
        .filter(|data| data.is_object())
        .and_then(|data| js_sys::Reflect::get(&data, &"platform".into()).ok())
        .and_then(|platform| platform.as_string())
        .filter(|platform| !platform.is_empty())
        .or_else(|| navigator.platform().ok())
}
