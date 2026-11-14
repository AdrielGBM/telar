use super::*;

fn entries(dir: &Path) -> usize {
    std::fs::read_dir(dir).unwrap().count()
}

#[test]
fn a_new_file_is_written_with_the_directories_above_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app").join("state").join("file");

    write_atomically(&path, b"contents").unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"contents");
    assert_eq!(entries(path.parent().unwrap()), 1);
}

#[test]
fn concurrent_writers_never_share_a_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shared");
    let handles: Vec<_> = (0..8)
        .map(|writer| {
            let path = path.clone();
            std::thread::spawn(move || {
                for round in 0..25 {
                    write_atomically(&path, format!("{writer} {round}").as_bytes()).unwrap();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }

    let last = std::fs::read_to_string(&path).unwrap();
    assert!(last.ends_with(" 24"), "{last}");
    assert_eq!(entries(dir.path()), 1);
}

#[test]
fn a_write_that_cannot_be_put_in_place_keeps_what_was_there_and_leaves_no_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    let occupied = dir.path().join("occupied");
    std::fs::create_dir(&occupied).unwrap();
    std::fs::write(occupied.join("kept"), "kept").unwrap();

    assert!(write_atomically(&occupied, b"replacement").is_err());

    assert_eq!(
        std::fs::read_to_string(occupied.join("kept")).unwrap(),
        "kept"
    );
    assert_eq!(entries(dir.path()), 1);
}

#[test]
fn a_file_a_reader_holds_open_is_still_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file");
    write_atomically(&path, b"before").unwrap();
    let reader = std::fs::File::open(&path).unwrap();

    write_atomically(&path, b"after").unwrap();

    drop(reader);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "after");
    assert_eq!(entries(dir.path()), 1);
}

#[test]
fn a_read_only_file_is_replaced_and_stays_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file");
    std::fs::write(&path, "before").unwrap();
    let mut read_only = std::fs::metadata(&path).unwrap().permissions();
    read_only.set_readonly(true);
    std::fs::set_permissions(&path, read_only).unwrap();

    write_atomically(&path, b"after").unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "after");
    assert!(std::fs::metadata(&path).unwrap().permissions().readonly());
    assert_eq!(entries(dir.path()), 1);
}

#[cfg(unix)]
#[test]
fn a_new_file_is_as_readable_as_one_written_in_place() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let in_place = dir.path().join("in-place");
    std::fs::write(&in_place, "").unwrap();
    let written = dir.path().join("written");

    write_atomically(&written, b"").unwrap();

    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode();
    assert_eq!(mode(&written), mode(&in_place));
}

#[cfg(unix)]
#[test]
fn writing_through_a_symlink_replaces_the_file_it_names_and_keeps_its_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("page.rsx");
    std::fs::write(&source, "before").unwrap();
    std::fs::set_permissions(&source, Permissions::from_mode(0o640)).unwrap();
    let link = dir.path().join("link.rsx");
    std::os::unix::fs::symlink(&source, &link).unwrap();

    write_atomically(&link, b"after").unwrap();

    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(std::fs::read_link(&link).unwrap(), source);
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "after");
    assert_eq!(
        std::fs::metadata(&source).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(entries(dir.path()), 2);
}
