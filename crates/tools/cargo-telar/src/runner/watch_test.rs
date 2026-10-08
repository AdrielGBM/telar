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

#[test]
fn the_applications_own_icons_are_watched() {
    let root = watch_probe("icons");
    std::fs::create_dir_all(root.join("icons/app")).unwrap();
    std::fs::write(
        root.join(telar_project::MANIFEST_FILENAME),
        "[telar.icons]\nsvg = \"icons\"\n",
    )
    .unwrap();

    let dirs = collect_watch_dirs(&root);

    assert!(dirs.contains(&root.join("icons")), "{dirs:?}");

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
fn modified(path: &Path) -> notify::Event {
    notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Content,
        )),
        paths: vec![path.to_path_buf()],
        attrs: Default::default(),
    }
}

#[test]
fn an_asset_extension_is_a_rebuild_event() {
    let watched = WatchSet {
        dirs: vec![PathBuf::from("/p/assets")],
        manifests: vec![],
    };
    assert!(
        watched.wants(&modified(Path::new("/p/assets/badge.webp"))),
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

#[test]
fn every_feature_the_loop_injects_is_one_a_project_can_lock() {
    for mode in [HotMode::Dev, HotMode::Preview] {
        for feature in mode.features().iter().chain(mode.hot_features()) {
            assert!(
                crate::runner::config::TOOLING_FEATURES.contains(feature),
                "`{feature}` is injected but missing from TOOLING_FEATURES"
            );
        }
    }
}

fn devtools_for(package: &str, devtools: Option<bool>) -> Option<String> {
    let resolved = resolve_package(&["-p", package].map(str::to_string));
    let mut config = TelarSection::default();
    config.dev.devtools = devtools;
    devtools_feature(&resolved, &config)
}

/// The overlay is the application's own optional dependency now, so the loop turns on the application's feature rather than one of `telar`'s.
#[test]
fn the_dev_loop_turns_on_the_devtools_an_app_declares() {
    for app in ["sandbox", "landing"] {
        assert_eq!(
            devtools_for(app, None),
            Some(format!("{app}/telar-devtools"))
        );
        assert_eq!(
            devtools_for(app, Some(true)),
            Some(format!("{app}/telar-devtools"))
        );
    }
}

/// `--devtools off` used to compile the overlay and hide it at runtime; now it is not compiled at all.
#[test]
fn devtools_off_leaves_the_overlay_out_of_the_build() {
    assert_eq!(devtools_for("sandbox", Some(false)), None);
}

/// Naming a feature the package does not have is a cargo error, so a package without the dependency builds as it always did.
#[test]
fn a_package_without_the_devtools_dependency_gets_no_feature() {
    assert_eq!(devtools_for("telar-components", None), None);
}

#[test]
fn the_devtools_feature_rides_along_with_the_telar_ones() {
    let features = with_devtools(HotMode::Dev.hot_features(), Some("sandbox/telar-devtools"));
    assert_eq!(
        features,
        ["telar/dev", "telar/hot-reload", "sandbox/telar-devtools"]
    );
    assert_eq!(
        with_devtools(HotMode::Dev.features(), None),
        HotMode::Dev.features()
    );
}

/// `prelude` and `theme` change the Rust every `.rsx` becomes, and the loop used to watch neither file: an edit there did nothing until some unrelated source was saved.
#[test]
fn every_telar_toml_a_member_reads_is_watched_through_its_directory() {
    let root = std::env::temp_dir().join(format!(
        "cargo_telar_watch_manifests_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let member = root.join("apps/site");
    std::fs::create_dir_all(member.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"apps/*\"]\n",
    )
    .unwrap();
    std::fs::write(member.join("Cargo.toml"), "[package]\nname = \"site\"\n").unwrap();
    std::fs::write(root.join("telar.toml"), "[telar]\n").unwrap();
    let root = root.canonicalize().unwrap();
    let member = root.join("apps/site");

    let watched = WatchSet::collect(&root);
    let watches = watched.watches();
    let _ = std::fs::remove_dir_all(&root);

    assert!(
        watched.manifests.contains(&root.join("telar.toml")),
        "the workspace's, which every member inherits from: {:?}",
        watched.manifests
    );
    assert!(
        watched.manifests.contains(&member.join("telar.toml")),
        "the member's own, even before it exists, so creating it is noticed: {:?}",
        watched.manifests
    );
    for dir in [&root, &member] {
        assert!(
            watches
                .iter()
                .any(|(watched, mode)| watched == dir && *mode == RecursiveMode::NonRecursive),
            "{dir:?} is watched for its manifest alone: {watches:?}"
        );
    }
    assert!(
        watches
            .iter()
            .any(|(watched, mode)| *watched == member.join("src")
                && *mode == RecursiveMode::Recursive),
        "{watches:?}"
    );
}

#[test]
fn a_telar_toml_edit_rebuilds_and_its_neighbours_do_not() {
    let watched = WatchSet {
        dirs: vec![PathBuf::from("/w/app/src")],
        manifests: vec![
            PathBuf::from("/w/telar.toml"),
            PathBuf::from("/w/app/telar.toml"),
        ],
    };
    assert!(watched.wants(&modified(Path::new("/w/telar.toml"))));
    assert!(watched.wants(&modified(Path::new("/w/app/telar.toml"))));
    assert!(watched.wants(&modified(Path::new("/w/app/src/home.rsx"))));
    assert!(
        !watched.wants(&modified(Path::new("/w/Cargo.toml"))),
        "a manifest's directory is watched for the manifest only"
    );
    assert!(!watched.wants(&modified(Path::new("/w/app/.telar.toml.swp"))));
}
