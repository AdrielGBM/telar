use super::*;

#[test]
fn a_name_is_a_lowercase_prefix_and_a_fixed_width_hash() {
    assert_eq!(
        file_name("arrow-right"),
        "arrow_right-88a9106abbf2a809f09bfacfce1eba3e"
    );
}

#[test]
fn ids_that_differ_only_in_case_do_not_share_a_file() {
    let upper = file_name("Logo");
    let lower = file_name("logo");
    assert_ne!(upper.to_lowercase(), lower.to_lowercase());
}

#[test]
fn ids_that_differ_only_in_punctuation_do_not_share_a_file() {
    assert_ne!(file_name("mdi:home"), file_name("mdi/home"));
}

#[test]
fn windows_reserved_names_are_not_produced() {
    for reserved in [
        "CON", "con", "NUL", "PRN", "AUX", "COM1", "LPT9", "con.svg", "nul.",
    ] {
        let name = file_name(reserved);
        let stem = name.split('.').next().unwrap().to_ascii_uppercase();
        let is_reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ((stem.starts_with("COM") || stem.starts_with("LPT")) && stem.len() == 4);
        assert!(!is_reserved, "{reserved} -> {name}");
    }
}

#[test]
fn an_id_spelling_another_ids_encoded_form_does_not_collide() {
    let encoded = file_name("logo");
    assert_ne!(file_name(&encoded), encoded);
}

#[test]
fn an_id_cannot_escape_the_cache_directory() {
    let name = file_name("../../etc/passwd");
    assert!(
        name.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "{name}"
    );
    assert_eq!(
        Path::new("/cache").join(&name).parent().unwrap(),
        Path::new("/cache")
    );
}

#[test]
fn an_empty_id_still_names_a_file_and_not_its_directory() {
    let cache = DiskCache::new("/cache");
    let path = cache.path(&AssetKey::new("svg", ""));
    assert_eq!(
        path.parent().unwrap().parent().unwrap(),
        Path::new("/cache")
    );
    assert!(path.file_name().is_some());
}

#[test]
fn two_formats_with_the_same_id_land_in_different_files() {
    let cache = DiskCache::new("/cache");
    let svg = cache.path(&AssetKey::new("svg", "logo"));
    let image = cache.path(&AssetKey::new("image", "logo"));
    assert_ne!(svg, image);
}
