use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("telar-prefs-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("app").join("preferences")
}

#[test]
fn a_file_store_keeps_what_it_was_told_for_the_next_run() {
    let path = scratch("keeps");
    let store = FileStore::open(&path);
    store.set("telar.scheme", Some("dark"));
    store.set("odd\tkey", Some("a line\nand a \\ back"));
    store.set("gone", Some("x"));
    store.set("gone", None);
    let next_run = FileStore::open(&path);
    assert_eq!(next_run.get("telar.scheme").as_deref(), Some("dark"));
    assert_eq!(
        next_run.get("odd\tkey").as_deref(),
        Some("a line\nand a \\ back")
    );
    assert_eq!(next_run.get("gone"), None);
}

#[test]
fn a_file_that_cannot_be_read_is_an_empty_store() {
    let store = FileStore::open(scratch("missing"));
    assert_eq!(store.get("anything"), None);
}

#[test]
fn without_a_store_installed_a_preference_holds_for_the_run() {
    store_preference("run.only", Some("yes"));
    assert_eq!(stored_preference("run.only").as_deref(), Some("yes"));
    store_preference("run.only", None);
    assert_eq!(stored_preference("run.only"), None);
}
