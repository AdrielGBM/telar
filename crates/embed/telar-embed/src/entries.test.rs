use super::*;

#[test]
fn the_table_starts_with_its_lifecycle_and_hashes_every_entry_with_what_it_takes() {
    platform_core::guest::check_entry_table::<EmbedEntries>();
    let due = EmbedEntries::LAYOUT
        .fields
        .iter()
        .find(|(field, _)| field.starts_with("next_timer_due:"))
        .expect("the timer entry is hashed");
    assert!(due.0.contains("Timestamp"), "{}", due.0);
}

#[test]
fn the_table_is_exported_under_the_symbol_its_host_reads() {
    assert_eq!(EmbedEntries::SYMBOL, b"telar_embed_entries\0");
}
