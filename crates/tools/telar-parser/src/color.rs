//! The one hex-colour table the whole toolchain reads.

/// Parses a hex colour body, with or without its `#`, into RGBA bytes.
///
/// The accepted lengths are **3, 4, 6 and 8** digits (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`); a short digit expands by `* 17`, so `#f` reads as `0xff`. This is the single statement of what a hex colour is: the `.rsx` front end validates against it, the transpiler lowers against it, the analyzer paints its swatch from it, and `geometry_core::Color::from_hex` accepts exactly the same set at runtime (it cannot depend on a tools crate, so it keeps its own body).
pub fn parse_hex(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |s: &str| u8::from_str_radix(s, 16).ok();
    let short = |s: &str| byte(s).map(|v| v * 17);
    Some(match hex.len() {
        3 => [
            short(&hex[0..1])?,
            short(&hex[1..2])?,
            short(&hex[2..3])?,
            255,
        ],
        4 => [
            short(&hex[0..1])?,
            short(&hex[1..2])?,
            short(&hex[2..3])?,
            short(&hex[3..4])?,
        ],
        6 => [byte(&hex[0..2])?, byte(&hex[2..4])?, byte(&hex[4..6])?, 255],
        8 => [
            byte(&hex[0..2])?,
            byte(&hex[2..4])?,
            byte(&hex[4..6])?,
            byte(&hex[6..8])?,
        ],
        _ => return None,
    })
}

#[cfg(test)]
#[path = "color_test.rs"]
mod tests;
