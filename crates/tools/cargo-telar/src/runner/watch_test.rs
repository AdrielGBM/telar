use super::*;

/// The linker a Nix or direnv shell configures for the host target used to be dropped on the floor: `RUSTFLAGS` was set here to carry a `--cfg`, and Cargo reads rustflags from exactly one tier — so the one loop a developer tunes their linker *for* was the one loop that ignored it. The flavour is a feature now and this loop sets no rustflags at all, which is what keeps the choice already made.
#[test]
fn the_hot_build_names_features_and_sets_no_rustflags() {
    assert!(
        HotMode::Dev.hot_features().contains(&"telar/hot-reload"),
        "the dylib half of the loop has to ask for the hot-reload shape"
    );
    assert!(
        !HotMode::Dev
            .hot_features()
            .iter()
            .any(|f| f.contains("cfg")),
        "a build shape carried in rustflags recompiles the whole graph on every switch"
    );
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

/// `telar/dev` used to carry `desktop-bare`, so every dev session linked a window — including the ones told to run in the terminal, where the host it belongs to is switched off before the build even starts. The frontend is the target's business now.
#[test]
fn the_dev_loop_names_no_frontend_of_its_own() {
    assert_eq!(HotMode::Dev.features(), &["telar/dev"]);
    assert!(
        !HotMode::Dev.hot_features().contains(&"telar/desktop-bare"),
        "the window the hot host opens comes with `telar/hot-reload`, which names it in the manifest"
    );
    assert!(
        HotMode::Preview.features().contains(&"telar/preview"),
        "the preview host is reached through its own feature whatever the target draws on"
    );
}

/// The two halves are swapped into one process, so a dylib built from another feature set is an application the host cannot drive. Only `-p` and `--release` used to come across.
#[test]
fn the_dylib_is_built_with_the_binarys_features() {
    let args = [
        "-p",
        "sandbox",
        "--no-default-features",
        "--features",
        "sandbox/tui",
    ]
    .map(str::to_string);

    let lib_args = make_lib_build_args(&args, &["telar/dev"]);

    assert!(
        lib_args.contains(&"--no-default-features".to_string()),
        "{lib_args:?}"
    );
    let features = lib_args
        .iter()
        .position(|arg| arg == "--features")
        .and_then(|pos| lib_args.get(pos + 1))
        .cloned()
        .unwrap_or_default();
    assert!(features.contains("sandbox/tui"), "{lib_args:?}");
    assert!(features.contains("telar/dev"), "{lib_args:?}");
}
