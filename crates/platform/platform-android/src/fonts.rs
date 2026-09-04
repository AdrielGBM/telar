//! Finding the system faces on Android, across the OEM font stacks that name them differently.

use std::path::PathBuf;

// The Android system font directory, scanned for fallback family resolution.
/// Where Android keeps its system faces.
pub fn system_fonts_dir() -> PathBuf {
    PathBuf::from("/system/fonts")
}

// Sans-serif family names to try, in priority order, across Android OEM font stacks (AOSP Roboto, legacy Droid, Xiaomi MiSans, Noto).
/// The sans-serif family names to try, in priority order across the OEM font stacks.
pub fn sans_serif_candidates() -> Vec<String> {
    vec![
        "Roboto".to_string(),
        "Droid Sans".to_string(),
        "MiSans Latin".to_string(),
        "Noto Sans".to_string(),
    ]
}
