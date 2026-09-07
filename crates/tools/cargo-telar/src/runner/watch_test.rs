use super::*;

/// The linker a Nix or direnv shell configures for the host target used to be dropped on the floor: setting `RUSTFLAGS` here made Cargo stop reading the tier that carried it, so the one loop a developer tunes their linker *for* was the one loop that ignored it. Choosing a linker is not this tool's call, but discarding the choice already made is a bug.
#[test]
fn a_shell_configured_linker_survives_into_the_hot_reload_build() {
    assert_eq!(
        with_hot_reload_cfg(Some("-C link-arg=-fuse-ld=mold".to_string())),
        "-C link-arg=-fuse-ld=mold --cfg=telar_hot_reload"
    );
}

/// With nothing to inherit the cfg stands alone, so a bare shell does not get a leading space that would read as an empty flag.
#[test]
fn nothing_to_inherit_leaves_the_cfg_alone() {
    assert_eq!(with_hot_reload_cfg(None), "--cfg=telar_hot_reload");
}
fn watch_probe(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("cargo_telar_watch_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["src", "assets", "locales"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"solo\"\n").unwrap();
    root
}

/// The bug this replaced: only `<member>/src` was watched, so editing `assets/badge.svg` or a translation raised no event at all.
#[test]
fn the_asset_and_locale_roots_are_watched_too() {
    let root = watch_probe("dirs");

    let dirs = collect_watch_dirs(&root);

    assert!(dirs.contains(&root.join("src")), "{dirs:?}");
    assert!(dirs.contains(&root.join("assets")), "{dirs:?}");
    assert!(dirs.contains(&root.join("locales")), "{dirs:?}");

    let _ = std::fs::remove_dir_all(&root);
}

/// A project from `cargo telar new` has no `[workspace]` table, and the old walk read members only — so it watched nothing whatsoever.
#[test]
fn a_lone_package_is_watched_at_all() {
    let root = watch_probe("lone");
    assert!(
        !collect_watch_dirs(&root).is_empty(),
        "a lone package still has directories worth watching"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Directories that do not exist are not handed to the watcher, which would only log a failure for each.
#[test]
fn absent_directories_are_left_out() {
    let root = watch_probe("absent");
    std::fs::remove_dir_all(root.join("locales")).unwrap();

    let dirs = collect_watch_dirs(&root);

    assert!(!dirs.contains(&root.join("locales")), "{dirs:?}");
    assert!(dirs.contains(&root.join("assets")), "{dirs:?}");

    let _ = std::fs::remove_dir_all(&root);
}

/// `[telar] assets` may point inside `src/`, and both watches are recursive — the inner one would make every edit arrive twice.
#[test]
fn a_directory_inside_another_is_not_watched_separately() {
    let root = watch_probe("nested");
    std::fs::create_dir_all(root.join("src/shared/assets")).unwrap();
    std::fs::write(
        root.join("telar.toml"),
        "[telar]\nassets = \"src/shared/assets\"\n",
    )
    .unwrap();

    let dirs = collect_watch_dirs(&root);

    assert!(dirs.contains(&root.join("src")), "{dirs:?}");
    assert!(
        !dirs.contains(&root.join("src/shared/assets")),
        "the asset root is already covered by the recursive watch on src/: {dirs:?}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// An asset edit has to reach the rebuild on its own now: nothing touches `.rsx` mtimes any more, and the `include_bytes!` the macro emits is what makes cargo see it.
#[test]
fn an_asset_extension_is_a_rebuild_event() {
    let event = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Content,
        )),
        paths: vec![PathBuf::from("/p/assets/badge.webp")],
        attrs: Default::default(),
    };
    assert!(
        note_event(&event),
        "an edited asset has to rebuild, or the baked artifact goes stale"
    );
}
