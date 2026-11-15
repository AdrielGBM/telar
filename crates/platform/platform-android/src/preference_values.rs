//! What Android's answers mean, kept apart from the JNI that fetches them so it can be tested off-device.

use platform_core::LocaleTag;

/// `LocaleList.toLanguageTags()`: BCP 47 tags joined by commas, most preferred first.
pub fn split_language_tags(tags: &str) -> Vec<LocaleTag> {
    platform_core::platform_locales(
        tags.split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty() && *tag != "und"),
    )
}

/// The one locale `AConfiguration` carries, for a device too old for `LocaleList`.
pub fn configuration_locale(language: Option<&str>, country: Option<&str>) -> Option<LocaleTag> {
    let language = language.filter(|l| !l.is_empty())?;
    let tag = match country.filter(|c| !c.is_empty()) {
        Some(country) => format!("{language}-{country}"),
        None => language.to_string(),
    };
    platform_core::platform_locales([tag]).pop()
}

/// "Remove animations" in the accessibility settings is `animator_duration_scale` set to 0; a slower or faster scale is a preference about speed, not a request for less motion.
pub fn animator_scale_reduces_motion(scale: f32) -> bool {
    scale == 0.0
}

#[cfg(test)]
#[path = "preference_values.test.rs"]
mod tests;
