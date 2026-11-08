use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cargo-telar-interrupt-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn an_interruption_removes_each_file_still_held_and_only_those() {
    let dir = scratch("held");
    let [held, kept, untracked] = ["held", "kept", "untracked"].map(|name| dir.join(name));
    for file in [&held, &kept, &untracked] {
        std::fs::write(file, "").unwrap();
    }
    let leftovers = Leftovers::new();
    leftovers.add(&held);
    leftovers.add(&kept);
    leftovers.keep(&kept);
    assert!(leftovers.holds(&held));
    assert!(!leftovers.holds(&kept));

    leftovers.remove_all();
    assert!(!held.exists());
    assert!(kept.exists());
    assert!(untracked.exists());
    assert!(!leftovers.holds(&held), "each file is removed once");
}

#[test]
fn a_file_already_gone_is_no_trouble() {
    let leftovers = Leftovers::new();
    leftovers.add(&scratch("gone").join("never-written"));
    leftovers.remove_all();
}
