//! Win32's wide strings: double-NUL-terminated lists, and the setting area a `WM_SETTINGCHANGE` names.

/// The strings in a `MULTI_SZ` buffer, stopping at the empty string that terminates it.
pub fn split_multi_sz(buffer: &[u16]) -> Vec<String> {
    buffer
        .split(|&unit| unit == 0)
        .take_while(|entry| !entry.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// Whether a `WM_SETTINGCHANGE` area string is `intl`, which Windows broadcasts when the regional and language settings change.
pub fn names_locale_settings(area: &[u16]) -> bool {
    String::from_utf16_lossy(area).eq_ignore_ascii_case("intl")
}

#[cfg(test)]
#[path = "wide_test.rs"]
mod tests;
