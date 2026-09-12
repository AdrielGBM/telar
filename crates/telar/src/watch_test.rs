use super::*;

/// The discriminator the platforms disagree about: a read leaves length and modification time alone, a write does not. Only inotify labels a read as `Access`; FSEvents coalesces and re-labels, so a watcher that trusted the label reported reads as changes there and passed on Linux.
#[test]
fn reading_leaves_the_fingerprint_alone_and_writing_moves_it() {
    let dir = std::env::temp_dir().join(format!("telar-fingerprint-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("nested")).unwrap();
    let file = dir.join("nested/held.toml");
    std::fs::write(&file, "a = 1\n").unwrap();

    let before = fingerprint(&dir);
    assert_eq!(before.len(), 1, "the file under the directory is stat'd");

    for _ in 0..20 {
        let _ = std::fs::read_to_string(&file).unwrap();
    }
    assert_eq!(fingerprint(&dir), before, "reading is not a change");

    std::fs::write(&file, "a = 22\n").unwrap();
    assert_ne!(fingerprint(&dir), before, "writing is");

    let _ = std::fs::remove_dir_all(&dir);
}
