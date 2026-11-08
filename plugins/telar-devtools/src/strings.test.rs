use super::*;

#[test]
fn the_catalog_is_sorted_so_every_key_is_found() {
    CATALOG.assert_sorted();
    for key in CATALOG.entries.iter().map(|entry| entry.key) {
        assert_ne!(text(key), key, "{key} resolves to its own key");
    }
}

#[test]
fn a_count_is_put_into_its_message() {
    assert_eq!(text_with(FPS, "60"), "60 fps");
    assert_eq!(text_with(NODES, "12"), "12 components");
    assert_eq!(text(BUILD_FAILED), "Build failed");
}
