//! Win32's wide strings: double-NUL-terminated lists, and the setting area a `WM_SETTINGCHANGE` names.

/// The strings in a `MULTI_SZ` buffer, stopping at the empty string that terminates it.
pub fn split_multi_sz(buffer: &[u16]) -> Vec<String> {
    buffer
        .split(|&unit| unit == 0)
        .take_while(|entry| !entry.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

const SPI_SETHIGHCONTRAST: u32 = 0x0043;
const SPI_SETCLIENTAREAANIMATION: u32 = 0x1043;

/// Whether a `WM_SETTINGCHANGE` area string is `intl`, which Windows broadcasts when the regional and language settings change.
fn names_locale_settings(area: &[u16]) -> bool {
    String::from_utf16_lossy(area).eq_ignore_ascii_case("intl")
}

/// Whether a `WM_SETTINGCHANGE` with this `wParam` action changes something the preferences carry: high contrast, client-area animation, or (by the area `area` reads, which is only fetched when the action does not decide it) the locale.
pub fn is_relevant_setting_change(action: u32, area: impl FnOnce() -> Vec<u16>) -> bool {
    matches!(action, SPI_SETCLIENTAREAANIMATION | SPI_SETHIGHCONTRAST)
        || names_locale_settings(&area())
}

#[cfg(test)]
#[path = "wide.test.rs"]
mod tests;
