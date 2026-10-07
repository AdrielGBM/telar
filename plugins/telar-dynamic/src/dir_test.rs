use super::*;

fn probe(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_dir_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("icons/mdi")).unwrap();
    std::fs::write(root.join("icons/mdi/home.svg"), b"<svg/>").unwrap();
    root
}

#[test]
fn an_id_keeps_its_subdirectories() {
    let root = probe("nested");
    let transport = DirTransport::new(&root);
    assert_eq!(transport.read("icons/mdi/home.svg").unwrap(), b"<svg/>");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_missing_file_is_an_error_and_not_a_panic() {
    let root = probe("missing");
    assert!(
        DirTransport::new(&root).read("icons/nope.svg").is_err(),
        "a missing file is an error, not a panic"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The first layer: rejected on spelling, before the filesystem is asked anything.
#[test]
fn an_id_cannot_climb_out_of_the_directory() {
    let root = probe("climb");
    let transport = DirTransport::new(&root);
    for id in ["../secret", "icons/../../secret", "/etc/passwd"] {
        let err = transport.read(id).unwrap_err();
        assert!(err.0.contains("not a path inside"), "{id}: {err}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// The second layer, and the reason there is one: this id is spelled like any other, and only the resolved path shows it leaves.
#[test]
#[cfg(unix)]
fn an_id_cannot_escape_through_a_symlink() {
    let root = probe("symlink");
    let outside = root.parent().unwrap().join("telar_dir_outside_secret");
    std::fs::write(&outside, b"secret").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();

    let err = DirTransport::new(&root).read("escape").unwrap_err();
    assert!(err.0.contains("resolves outside"), "{err}");

    let _ = std::fs::remove_file(&outside);
    let _ = std::fs::remove_dir_all(&root);
}
