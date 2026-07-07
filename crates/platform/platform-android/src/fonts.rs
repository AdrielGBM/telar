use std::path::PathBuf;

// The Android system font directory, scanned for fallback family resolution.
pub fn system_fonts_dir() -> PathBuf {
    PathBuf::from("/system/fonts")
}

// Sans-serif family names to try, in priority order, across Android OEM font stacks (AOSP Roboto,
// legacy Droid, Xiaomi MiSans, Noto).
pub fn sans_serif_candidates() -> Vec<String> {
    vec![
        "Roboto".to_string(),
        "Droid Sans".to_string(),
        "MiSans Latin".to_string(),
        "Noto Sans".to_string(),
    ]
}
