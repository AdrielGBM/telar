use super::*;

#[test]
fn a_plain_id_is_its_own_file_name() {
    assert_eq!(file_name("arrow-right"), "arrow-right");
}

#[test]
fn ids_that_differ_only_in_punctuation_do_not_share_a_file() {
    let colon = file_name("mdi:home");
    let slash = file_name("mdi/home");
    assert!(colon.starts_with("mdi_home_"), "{colon}");
    assert_ne!(colon, slash);
}

/// A separator in an id must not become a separator in the path, or the cache writes outside itself.
#[test]
fn an_id_cannot_escape_the_cache_directory() {
    let name = file_name("../../etc/passwd");
    assert!(!name.contains('/'), "{name}");
    assert!(!name.contains(".."), "{name}");
    assert_eq!(
        Path::new("/cache").join(&name).parent().unwrap(),
        Path::new("/cache")
    );
}

/// An empty id used to be harmless because the name always ended in `.svg`; with the extension gone it would name the kind directory itself.
#[test]
fn an_empty_id_still_names_a_file_and_not_its_directory() {
    let cache = DiskCache::new("/cache");
    let path = cache.path(&AssetKey::new("svg", ""));
    assert_eq!(path.parent().unwrap(), Path::new("/cache/svg"));
    assert!(
        path.file_name().is_some(),
        "an empty id must still name a file: {}",
        path.display()
    );
}

/// The collision the `kind` segment exists to prevent: the same id under two decoders is two files.
#[test]
fn two_formats_with_the_same_id_land_in_different_files() {
    let cache = DiskCache::new("/cache");
    let svg = cache.path(&AssetKey::new("svg", "logo"));
    let image = cache.path(&AssetKey::new("image", "logo"));
    assert_eq!(svg, Path::new("/cache/svg/logo"));
    assert_ne!(svg, image);
}
