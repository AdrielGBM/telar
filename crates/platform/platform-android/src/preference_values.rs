//! What Android's answers mean, kept apart from the JNI that fetches them so it can be tested off-device.

/// `LocaleList.toLanguageTags()`: BCP 47 tags joined by commas, most preferred first.
pub fn split_language_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty() && *tag != "und")
        .map(str::to_owned)
        .collect()
}

/// The one locale `AConfiguration` carries, for a device too old for `LocaleList`.
pub fn configuration_locale(language: Option<&str>, country: Option<&str>) -> Option<String> {
    let language = language.filter(|l| !l.is_empty())?.to_ascii_lowercase();
    Some(match country.filter(|c| !c.is_empty()) {
        Some(country) => format!("{language}-{}", country.to_ascii_uppercase()),
        None => language,
    })
}

/// "Remove animations" in the accessibility settings is `animator_duration_scale` set to 0; a slower or faster scale is a preference about speed, not a request for less motion.
pub fn animator_scale_reduces_motion(scale: f32) -> bool {
    scale == 0.0
}

#[cfg(test)]
#[path = "preference_values_test.rs"]
mod tests;
