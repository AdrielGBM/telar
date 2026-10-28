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

    let dirs = collect_watch_dirs(&root).unwrap();

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

    let dirs = collect_watch_dirs(&root).unwrap();

    assert!(dirs.contains(&root.join("icons")), "{dirs:?}");

    let _ = std::fs::remove_dir_all(&root);
}

/// A project from `cargo telar new` has no `[workspace]` table, and the old walk read members only — so it watched nothing whatsoever.
#[test]
fn a_lone_package_is_watched_at_all() {
    let root = watch_probe("lone");
    assert!(
        !collect_watch_dirs(&root).unwrap().is_empty(),
        "a lone package still has directories worth watching"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_telar_toml_that_does_not_load_is_an_error_rather_than_the_default_directories() {
    let root = watch_probe("broken");
    std::fs::write(
        root.join(telar_project::MANIFEST_FILENAME),
        "[telar]\nasets = \"art\"\n",
    )
    .unwrap();

    let error = collect_watch_dirs(&root).unwrap_err();

    assert!(error.to_string().contains("asets"), "{error}");
    let _ = std::fs::remove_dir_all(&root);
}

/// Directories that do not exist are not handed to the watcher, which would only log a failure for each.
#[test]
fn absent_directories_are_left_out() {
    let root = watch_probe("absent");
    std::fs::remove_dir_all(root.join("locales")).unwrap();

    let dirs = collect_watch_dirs(&root).unwrap();

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

    let dirs = collect_watch_dirs(&root).unwrap();

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
        let injected = mode.features().iter().chain(mode.hot_features());
        for feature in injected.chain(mode.web_features()) {
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
fn the_tooling_features_ride_along_with_the_telar_ones() {
    let features = with_tooling(
        HotMode::Preview.hot_features(),
        &["sandbox/telar-devtools", "sandbox/telar-workshop"],
    );
    assert_eq!(
        features,
        [
            "telar/preview",
            "telar/dev",
            "telar/hot-reload",
            "sandbox/telar-devtools",
            "sandbox/telar-workshop"
        ]
    );
    assert_eq!(
        with_tooling(HotMode::Dev.features(), &[]),
        HotMode::Dev.features()
    );
}

fn workshop_for(mode: HotMode, package: &str) -> Option<String> {
    workshop_feature(
        &mode,
        &resolve_package(&["-p", package].map(str::to_string)),
    )
}

/// A dev session never builds the workshop, and a package that does not declare it falls back to the plain page rather than naming a feature cargo would refuse.
#[test]
fn the_workshop_is_turned_on_only_by_a_preview_of_a_package_that_declares_it() {
    assert_eq!(workshop_for(HotMode::Dev, "sandbox"), None);
    assert_eq!(workshop_for(HotMode::Preview, "telar-components"), None);
}

/// A page loads no dylib, so `telar/dev` would only fail to build for wasm; and it draws no overlay, so the devtools would be dead weight.
#[test]
fn a_web_preview_builds_the_preview_host_and_the_workshop_alone() {
    let args = with_web_features(
        &HotMode::Preview,
        ["-p", "catalogue"].map(str::to_string).to_vec(),
    );
    assert_eq!(
        args,
        [
            "-p",
            "catalogue",
            "--features",
            "telar/preview,catalogue/telar-workshop"
        ]
    );
    let dev = ["-p", "catalogue"].map(str::to_string).to_vec();
    assert_eq!(with_web_features(&HotMode::Dev, dev.clone()), dev);
}

#[test]
fn the_fallback_hint_names_the_command_that_adds_the_workshop() {
    let note = missing_workshop_note("app");
    assert!(
        note.contains("cargo add -p app telar-workshop --optional"),
        "{note}"
    );
    assert_eq!(note.lines().count(), 1, "{note}");
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

    let watched = WatchSet::collect(&root).unwrap();
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

#[test]
fn the_app_is_handed_its_own_arguments_after_cargo_s() {
    let args = vec!["--location".to_string(), "/preview/a".to_string()];
    assert_eq!(
        with_app_args(vec!["run".into(), "-p".into(), "demo".into()], &args),
        ["run", "-p", "demo", "--", "--location", "/preview/a"]
    );
    assert_eq!(
        with_app_args(vec!["run".into(), "--".into(), "--verbose".into()], &args),
        ["run", "--", "--verbose", "--location", "/preview/a"]
    );
    assert_eq!(with_app_args(vec!["run".into()], &[]), ["run"]);
}

/// A scope that wants every ordinary event or none, and watches nothing.
struct Wanting(bool);

impl WatchScope for Wanting {
    fn watches(&self) -> Vec<(PathBuf, RecursiveMode)> {
        Vec::new()
    }

    fn wants(&self, _: &notify::Event) -> bool {
        self.0
    }
}

fn feed(
    wanting: bool,
) -> (
    mpsc::Sender<notify::Result<notify::Event>>,
    ChangeFeed<Wanting>,
) {
    let (tx, messages) = mpsc::channel();
    let (unused, _) = mpsc::channel();
    let watcher = RecommendedWatcher::new(unused, NotifyConfig::default()).unwrap();
    let feed = ChangeFeed {
        scope: Wanting(wanting),
        messages,
        _watcher: watcher,
    };
    (tx, feed)
}

fn rescan() -> notify::Event {
    notify::Event::new(EventKind::Other).set_flag(notify::event::Flag::Rescan)
}

/// The loops used to act on `Ok(event)` alone, so a rescan notice — the platform saying it dropped events, an edit among them maybe — was ignored and the rebuild never came.
#[test]
fn a_rescan_notice_is_a_change_whatever_the_scope_wants() {
    let (tx, changes) = feed(false);
    tx.send(Ok(rescan())).unwrap();
    assert!(changes.drain());
    tx.send(Ok(rescan())).unwrap();
    assert!(changes.changed_within(Duration::from_secs(5)));
}

#[test]
fn a_watcher_error_is_not_a_change_and_does_not_stop_the_feed() {
    let (tx, changes) = feed(true);
    tx.send(Err(notify::Error::generic("queue overflow")))
        .unwrap();
    assert!(!changes.drain());
    tx.send(Err(notify::Error::generic("queue overflow")))
        .unwrap();
    tx.send(Ok(modified(Path::new("/p/src/lib.rs")))).unwrap();
    assert!(changes.changed_within(Duration::from_secs(5)));
}

#[test]
fn draining_takes_every_waiting_message() {
    let (tx, changes) = feed(true);
    tx.send(Err(notify::Error::generic("lost"))).unwrap();
    tx.send(Ok(modified(Path::new("/p/src/a.rs")))).unwrap();
    tx.send(Err(notify::Error::generic("lost"))).unwrap();
    assert!(changes.drain());
    assert!(!changes.drain(), "nothing is left for the next look");
}

#[test]
fn an_ignored_event_is_not_a_change() {
    let (tx, changes) = feed(false);
    tx.send(Ok(modified(Path::new("/p/notes.txt")))).unwrap();
    assert!(!changes.drain());
    assert!(!changes.changed_within(Duration::from_millis(10)));
}

#[test]
fn the_inotify_limit_is_reported_with_the_path_and_how_to_raise_it() {
    let error = notify::Error::new(notify::ErrorKind::MaxFilesWatch).add_path("/w/app/src".into());
    let message = watch_failure(&error);
    assert!(message.contains("/w/app/src"), "{message}");
    assert!(message.contains("fs.inotify.max_user_watches"), "{message}");
}

#[test]
fn a_watcher_failure_without_a_path_still_says_why() {
    let message = watch_failure(&notify::Error::generic("inotify read failed"));
    assert!(message.contains("inotify read failed"), "{message}");
}

#[test]
fn a_directory_that_cannot_be_watched_does_not_stop_the_others() {
    struct Missing;
    impl WatchScope for Missing {
        fn watches(&self) -> Vec<(PathBuf, RecursiveMode)> {
            vec![(
                PathBuf::from("/nonexistent/cargo-telar-watch"),
                RecursiveMode::Recursive,
            )]
        }
        fn wants(&self, _: &notify::Event) -> bool {
            false
        }
    }
    assert!(ChangeFeed::start(Missing).is_ok());
}

/// The bug this replaced: a first build that failed with nothing built before dropped the session into restart mode for good, hot reload off.
#[test]
fn a_failed_first_build_is_retried_on_each_change_until_one_succeeds() {
    let (tx, changes) = feed(true);
    tx.send(Ok(modified(Path::new("/p/src/lib.rs")))).unwrap();
    let mut attempts = 0;
    rebuild_until_built(&changes, || {
        attempts += 1;
        if attempts == 1 {
            tx.send(Ok(modified(Path::new("/p/src/lib.rs")))).unwrap();
            return false;
        }
        true
    });
    assert_eq!(attempts, 2);
}

fn connect_to(channel: &HotChannel, opening: Option<&str>) -> TcpStream {
    use std::io::Write;
    let mut stream = TcpStream::connect(("127.0.0.1", channel.port)).unwrap();
    match opening {
        Some(line) => writeln!(stream, "{line}").unwrap(),
        None => stream.shutdown(std::net::Shutdown::Write).unwrap(),
    }
    stream
}

fn adopted_within(channel: &mut HotChannel, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        channel.accept_waiting();
        if channel.stream.is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn first_line(stream: TcpStream) -> String {
    use std::io::BufRead;
    match stream.set_read_timeout(Some(Duration::from_secs(5))) {
        Ok(()) => {}
        // macOS refuses socket options with EINVAL once the peer has torn the connection down, which is the hang-up the callers look for.
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => return String::new(),
        Err(error) => panic!("could not set a read timeout: {error}"),
    }
    let mut line = String::new();
    let _ = BufReader::new(stream).read_line(&mut line);
    line
}

/// Any local process can reach a loopback port, and the channel used to adopt whichever connected last: it could load a dylib of its choosing into the app.
#[test]
fn a_connection_without_the_token_is_never_adopted() {
    let mut channel = HotChannel::bind();
    let silent = connect_to(&channel, None);
    let wrong = connect_to(&channel, Some("not-the-token"));
    let prefixed = connect_to(&channel, Some(&format!("{}x", channel.token)));
    assert!(!adopted_within(&mut channel, Duration::from_millis(300)));
    for stream in [silent, wrong, prefixed] {
        assert_eq!(first_line(stream), "", "an impostor is hung up on");
    }
}

#[test]
fn the_connection_opening_with_the_token_is_adopted_and_later_ones_are_not() {
    let mut channel = HotChannel::bind();
    let impostor = connect_to(&channel, Some("guess"));
    let token = channel.token.clone();
    let app = connect_to(&channel, Some(&token));
    assert!(adopted_within(&mut channel, Duration::from_secs(5)));

    let deadline = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(("127.0.0.1", channel.port)).is_ok() {
        assert!(
            Instant::now() < deadline,
            "the listener closes once the app is in, so even the token cannot take the channel over"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(channel.deliver("focus"));
    assert_eq!(first_line(app), "focus\n");
    assert_eq!(first_line(impostor), "");
}

#[test]
fn every_session_draws_its_own_token() {
    let (a, b) = (hot_token(), hot_token());
    assert_eq!(a.len(), 64);
    assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_ne!(a, b);
}

const LIBRARY: &str = "/t/libapp.so";

fn reloads() -> (mpsc::Sender<Heard>, Reloads) {
    let (heard_tx, heard) = mpsc::channel();
    (heard_tx, Reloads::new(heard))
}

fn reply(library: &str, outcome: HotOutcome) -> Heard {
    Heard::Reply(HotReply {
        library: library.to_owned(),
        outcome,
    })
}

#[test]
fn a_reload_is_called_loaded_only_once_the_app_says_so() {
    let (heard_tx, mut reloads) = reloads();
    let now = Instant::now();
    heard_tx.send(Heard::AnswersReloads).unwrap();
    reloads.sent(LIBRARY, now);
    assert_eq!(
        reloads.settle(now),
        [],
        "nothing is claimed before the app answers"
    );

    heard_tx.send(reply(LIBRARY, HotOutcome::Loaded)).unwrap();
    let outcomes = reloads.settle(now);
    assert_eq!(outcomes, [ReloadOutcome::Answered(HotOutcome::Loaded)]);
    assert_eq!(outcomes[0].message(), "Hot reloaded.");
    assert_eq!(
        reloads.settle(now + REPLY_TIMEOUT),
        [],
        "an answered reload is settled"
    );
}

#[test]
fn a_refused_reload_is_said_with_the_app_s_reason() {
    let (heard_tx, mut reloads) = reloads();
    let now = Instant::now();
    reloads.sent(LIBRARY, now);
    let reason =
        "/t/libapp.so: it was built against another telar than this host: restart the session";
    heard_tx
        .send(reply(LIBRARY, HotOutcome::Refused(reason.to_owned())))
        .unwrap();
    let outcomes = reloads.settle(now);
    let message = outcomes[0].message();
    assert!(!message.contains("Hot reloaded"), "{message}");
    assert!(message.contains("refused"), "{message}");
    assert!(message.ends_with(reason), "{message}");
}

#[test]
fn a_reload_whose_app_panicked_as_it_started_is_said_to_have_loaded_and_panicked() {
    let (heard_tx, mut reloads) = reloads();
    let now = Instant::now();
    reloads.sent(LIBRARY, now);
    let report = "panicked at src/app.rs:4:9:\nthe app's setup could not find its assets";
    heard_tx
        .send(reply(LIBRARY, HotOutcome::Panicked(report.to_owned())))
        .unwrap();
    let outcomes = reloads.settle(now);
    assert_eq!(
        outcomes,
        [ReloadOutcome::Answered(HotOutcome::Panicked(
            report.to_owned()
        ))]
    );
    let message = outcomes[0].message();
    assert!(!message.contains("Hot reloaded"), "{message}");
    assert!(
        message.contains("loaded, but the app panicked while starting"),
        "{message}"
    );
    assert!(message.ends_with(report), "{message}");
    assert_eq!(reloads.settle(now + REPLY_TIMEOUT), [], "it is answered");
}

#[test]
fn a_reload_left_unanswered_is_said_to_be_once_the_wait_is_over_and_a_late_answer_still_counts() {
    let (heard_tx, mut reloads) = reloads();
    let now = Instant::now();
    heard_tx.send(Heard::AnswersReloads).unwrap();
    reloads.sent(LIBRARY, now);
    assert_eq!(reloads.settle(now + REPLY_TIMEOUT / 2), []);
    let outcomes = reloads.settle(now + REPLY_TIMEOUT);
    assert_eq!(outcomes, [ReloadOutcome::Unanswered { app_answers: true }]);
    assert!(!outcomes[0].message().contains("Hot reloaded"));
    assert_eq!(reloads.settle(now + REPLY_TIMEOUT), [], "said once");

    heard_tx.send(reply(LIBRARY, HotOutcome::Loaded)).unwrap();
    assert_eq!(
        reloads.settle(now + REPLY_TIMEOUT * 2),
        [ReloadOutcome::Answered(HotOutcome::Loaded)]
    );
}

/// A late answer settles the reload it names, so the reload sent after it is still waited on, and still said to be unanswered if it is.
#[test]
fn a_late_answer_settles_the_reload_it_names_and_not_the_next() {
    let (heard_tx, mut reloads) = reloads();
    let start = Instant::now();
    heard_tx.send(Heard::AnswersReloads).unwrap();
    reloads.sent("/t/first/libapp.so", start);
    assert_eq!(
        reloads.settle(start + REPLY_TIMEOUT),
        [ReloadOutcome::Unanswered { app_answers: true }]
    );

    let next = start + REPLY_TIMEOUT * 2;
    reloads.sent("/t/second/libapp.so", next);
    heard_tx
        .send(reply("/t/first/libapp.so", HotOutcome::Loaded))
        .unwrap();
    assert_eq!(
        reloads.settle(next),
        [ReloadOutcome::Answered(HotOutcome::Loaded)]
    );
    assert_eq!(
        reloads.settle(next + REPLY_TIMEOUT),
        [ReloadOutcome::Unanswered { app_answers: true }],
        "the second reload is still unanswered"
    );
}

/// Every rebuild leaves its library at the same path, so the answers to it settle the reloads in the order they were sent.
#[test]
fn answers_naming_the_same_library_settle_the_oldest_reload_first() {
    let (heard_tx, mut reloads) = reloads();
    let start = Instant::now();
    heard_tx.send(Heard::AnswersReloads).unwrap();
    reloads.sent(LIBRARY, start);
    reloads.settle(start + REPLY_TIMEOUT);

    let next = start + REPLY_TIMEOUT * 2;
    reloads.sent(LIBRARY, next);
    heard_tx.send(reply(LIBRARY, HotOutcome::Loaded)).unwrap();
    reloads.settle(next);
    assert_eq!(
        reloads.settle(next + REPLY_TIMEOUT),
        [ReloadOutcome::Unanswered { app_answers: true }],
        "the late answer was the first reload's"
    );
}

/// An app built before reload answers never says it answers, so its silence is not news about the reload.
#[test]
fn an_app_that_never_said_it_answers_is_not_waited_on_as_if_it_had() {
    let (_heard_tx, mut reloads) = reloads();
    let now = Instant::now();
    reloads.sent(LIBRARY, now);
    let outcomes = reloads.settle(now + REPLY_TIMEOUT);
    assert_eq!(outcomes, [ReloadOutcome::Unanswered { app_answers: false }]);
    assert!(
        outcomes[0].message().contains("does not say"),
        "{}",
        outcomes[0].message()
    );
    assert_eq!(reloads.finish(), [], "nothing is left waiting on it");
}

#[test]
fn a_reload_the_app_closed_on_is_said_to_be_unanswered_at_once() {
    let (heard_tx, mut reloads) = reloads();
    let now = Instant::now();
    reloads.sent(LIBRARY, now);
    drop(heard_tx);
    let outcomes = reloads.settle(now);
    assert_eq!(outcomes, [ReloadOutcome::ChannelClosed]);
    assert!(!outcomes[0].message().contains("Hot reloaded"));
}

#[test]
fn an_app_that_exits_leaves_its_unanswered_reload_unanswered() {
    let (heard_tx, mut reloads) = reloads();
    reloads.sent(LIBRARY, Instant::now());
    heard_tx.send(reply(LIBRARY, HotOutcome::Loaded)).unwrap();
    reloads.sent(LIBRARY, Instant::now());
    assert_eq!(
        reloads.finish(),
        [
            ReloadOutcome::Answered(HotOutcome::Loaded),
            ReloadOutcome::ChannelClosed
        ]
    );
    assert_eq!(reloads.finish(), [], "nothing was sent since");
}

#[test]
fn only_the_lines_of_this_version_of_the_answers_are_heard() {
    assert_eq!(heard_from(HOT_REPLIES_LINE), Some(Heard::AnswersReloads));
    assert_eq!(
        heard_from("loaded:/t/libapp.so"),
        Some(reply(LIBRARY, HotOutcome::Loaded))
    );
    assert_eq!(heard_from("replies:1"), None);
    assert_eq!(heard_from("loaded"), None);
    assert_eq!(heard_from("hot:/t/libapp.so"), None);
}

/// What the app writes back on the adopted connection reaches the loop, framed as the app frames it, and settles the reload it names.
#[test]
fn the_app_s_answer_to_a_reload_comes_back_over_the_authenticated_channel() {
    use std::io::Write;
    let mut channel = HotChannel::bind();
    let impostor = connect_to(&channel, Some("guess"));
    let token = channel.token.clone();
    let mut app = connect_to(&channel, Some(&token));
    app.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    writeln!(app, "{HOT_REPLIES_LINE}").unwrap();
    assert!(adopted_within(&mut channel, Duration::from_secs(5)));
    drop(impostor);

    channel.notify_hot_reload(LIBRARY);
    let mut reader = BufReader::new(app.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert_eq!(line, "hot:/t/libapp.so\n");
    let report = "panicked at src/app.rs:4:9:\nthe app's setup could not find its assets";
    let answer = HotReply {
        library: LIBRARY.to_owned(),
        outcome: HotOutcome::Panicked(report.to_owned()),
    };
    writeln!(app, "{}", answer.line()).unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    let outcomes = loop {
        let outcomes = channel.reloads.settle(Instant::now());
        if !outcomes.is_empty() || Instant::now() >= deadline {
            break outcomes;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        outcomes,
        [ReloadOutcome::Answered(HotOutcome::Panicked(
            report.to_owned()
        ))]
    );
    assert!(channel.reloads.app_answers);
    assert!(channel.reloads.pending.is_empty(), "the reload is settled");
}
