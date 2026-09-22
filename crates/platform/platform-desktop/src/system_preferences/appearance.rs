//! The freedesktop `org.freedesktop.appearance` settings, as values rather than D-Bus variants.

use platform_core::{ColorScheme, SystemPreferences};

/// One appearance key the portal reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppearanceSetting {
    ColorScheme(Option<ColorScheme>),
    HighContrast(Option<bool>),
    ReducedMotion(Option<bool>),
}

impl AppearanceSetting {
    /// Interprets `key` = `value`, or `None` for a key this model does not carry (`accent-color`, and whatever a later portal adds).
    ///
    /// `color-scheme` 0 is "no preference", which is not a vote for light, so it stays unknown. For `contrast` and `reduced-motion` 0 is the user not having asked, which is a real `false`. A value outside the spec is unknown rather than guessed at.
    pub fn parse(key: &str, value: u32) -> Option<Self> {
        match key {
            "color-scheme" => Some(Self::ColorScheme(match value {
                1 => Some(ColorScheme::Dark),
                2 => Some(ColorScheme::Light),
                _ => None,
            })),
            "contrast" => Some(Self::HighContrast(flag(value))),
            "reduced-motion" => Some(Self::ReducedMotion(flag(value))),
            _ => None,
        }
    }

    pub fn apply(self, preferences: &mut SystemPreferences) {
        match self {
            Self::ColorScheme(scheme) => preferences.color_scheme = scheme,
            Self::HighContrast(on) => preferences.high_contrast = on,
            Self::ReducedMotion(on) => preferences.reduced_motion = on,
        }
    }
}

fn flag(value: u32) -> Option<bool> {
    match value {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// Every appearance key the portal answered, applied over a snapshot whose fields start unknown.
pub fn from_settings<'a>(settings: impl IntoIterator<Item = (&'a str, u32)>) -> SystemPreferences {
    let mut preferences = SystemPreferences::default();
    for (key, value) in settings {
        if let Some(setting) = AppearanceSetting::parse(key, value) {
            setting.apply(&mut preferences);
        }
    }
    preferences
}

#[cfg(test)]
#[path = "appearance_test.rs"]
mod tests;
