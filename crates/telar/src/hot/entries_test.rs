use super::*;

#[test]
fn the_table_starts_with_its_lifecycle_and_hashes_every_entry_with_what_it_takes() {
    platform_core::guest::check_entry_table::<HotEntries>();
    let paint = HotEntries::LAYOUT
        .fields
        .iter()
        .find(|(field, _)| field.starts_with("tree_paint:"))
        .expect("the paint entry is hashed");
    assert!(paint.0.contains("LentBytes"), "{}", paint.0);
}

#[test]
fn the_table_is_exported_under_the_symbol_its_host_reads() {
    assert_eq!(HotEntries::SYMBOL, b"telar_hot_entries\0");
}
