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

#[test]
fn a_file_is_watched_through_its_parent_and_a_directory_directly() {
    let dir = std::env::temp_dir().join(format!("telar-target-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("held.toml");

    let for_file = target(&file).unwrap();
    assert_eq!(for_file.watched, dir);
    assert!(matches!(for_file.mode, RecursiveMode::NonRecursive));
    assert_eq!(for_file.file_name.as_deref(), Some(OsStr::new("held.toml")));

    let for_dir = target(&dir).unwrap();
    assert_eq!(for_dir.watched, dir);
    assert!(matches!(for_dir.mode, RecursiveMode::Recursive));

    assert!(target(&dir.join("absent/held.toml")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
