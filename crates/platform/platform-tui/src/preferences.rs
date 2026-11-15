//! What a terminal can say about the user's preferences without a round trip: the locale its environment declares, and a background colour hint.
//!
//! Motion and contrast are the terminal emulator's business and reach no program running inside it, so they stay unknown; its cells are its own size, so text stays unscaled.

use platform_core::{ColorScheme, SystemPreferences, TextScale};

pub fn read() -> SystemPreferences {
    SystemPreferences {
        color_scheme: std::env::var("COLORFGBG")
            .ok()
            .as_deref()
            .and_then(colorfgbg_scheme),
        reduced_motion: None,
        high_contrast: None,
        locales: platform_core::system_locales_from_env(),
        text_scale: TextScale::UNSCALED,
    }
}

/// `COLORFGBG`'s last field is the background's palette index: the dark half of the 16 colours, plus 8 (grey), is a dark background.
pub fn colorfgbg_scheme(value: &str) -> Option<ColorScheme> {
    let index: u8 = value.rsplit(';').next()?.trim().parse().ok()?;
    Some(if matches!(index, 0..=6 | 8) {
        ColorScheme::Dark
    } else {
        ColorScheme::Light
    })
}

#[cfg(test)]
#[path = "preferences.test.rs"]
mod tests;
