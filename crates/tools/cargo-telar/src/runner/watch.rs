//! `cargo telar dev`: the watch loop, the rebuild, and the hot-reload channel to the running app.

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use telar_project::protocol::{BUILD_ERROR_PREFIX, HOT_RELOAD_PREFIX, WORKSPACE_DIR_VAR};
use telar_project::{ASSET_KINDS, DEVTOOLS_PACKAGE, WORKSHOP_PACKAGE};

use super::android::{android_install_and_launch, make_android_cmd};
use super::config::{
    ResolvedPackage, TelarSection, WindowSection, backend_as_str, missing_devtools_note,
    missing_workshop_note, resolve_package, split_android_flag, warn_if_tooling_unlocked,
};
use super::diagnostics;
use super::package::{package_bin_path, package_lib_path, profile_of};
use super::workshop_channel::{WorkshopChannel, channel_file};

fn inject_feature(args: &mut Vec<String>, feature: &str) {
    if let Some(pos) = args.iter().position(|a| a == "--features" || a == "-F")
        && pos + 1 < args.len()
    {
        args[pos + 1] = format!("{},{feature}", args[pos + 1]);
        return;
    }
    args.push("--features".to_string());
    args.push(feature.to_string());
}

/// Runs a cargo build and re-points whatever it says about generated Rust back onto the `.rsx` it came from.
///
/// stdout carries the JSON diagnostic stream and is consumed here; stderr is cargo's own progress and is left on the terminal, so a build still looks like a build. Every build goes through this, not only the failing ones — warnings used to be captured into a `String` that was read on the failure path alone, which made them invisible for a whole development session.
fn build_with_diagnostics(cmd: &mut Command) -> (bool, diagnostics::Report) {
    cmd.arg("--color=always")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = cmd.spawn().expect("[cargo-telar] failed to invoke cargo");
    // Drained before `wait`, or a report larger than the pipe buffer deadlocks the build it is reading.
    let report = match child.stdout.take() {
        Some(stdout) => diagnostics::collect(BufReader::new(stdout)),
        None => diagnostics::Report::default(),
    };
    let succeeded = child.wait().map(|status| status.success()).unwrap_or(false);
    (succeeded, report)
}

/// Adds `--message-format=json` unless the caller already chose a format, which cargo would reject as two of them rather than honour the later one.
fn with_json_messages(args: &mut Vec<String>) {
    if !args.iter().any(|a| a.starts_with("--message-format")) {
        args.push("--message-format=json".to_string());
    }
}

/// The dylib build, from the binary's own invocation.
///
/// Every flag that decides a feature has to come along: the two halves are swapped into one process, and a dylib built from another feature set is an application the host cannot drive. It used to copy `-p` and `--release` alone, so `--target tui` or a `-F` on the command line reached the binary and not the library beside it.
fn make_lib_build_args(args: &[String], features: &[&str]) -> Vec<String> {
    let mut lib_build_args = vec!["build".to_string(), "--lib".to_string()];
    for pair in args.windows(2) {
        if ["-p", "--package", "--features", "-F"].contains(&pair[0].as_str()) {
            lib_build_args.push(pair[0].clone());
            lib_build_args.push(pair[1].clone());
        }
    }
    for flag in ["--release", "--no-default-features"] {
        if args.iter().any(|arg| arg == flag) {
            lib_build_args.push(flag.to_string());
        }
    }
    for feature in features {
        inject_feature(&mut lib_build_args, feature);
    }
    with_json_messages(&mut lib_build_args);
    lib_build_args
}

fn apply_dev_window_env(envs: &mut Vec<(String, String)>, window: &WindowSection) {
    if let Some(title) = &window.title {
        envs.push(("TELAR_DEV_WINDOW_TITLE".to_string(), title.clone()));
    }
    if let Some(width) = window.width {
        envs.push(("TELAR_DEV_WINDOW_WIDTH".to_string(), width.to_string()));
    }
    if let Some(height) = window.height {
        envs.push(("TELAR_DEV_WINDOW_HEIGHT".to_string(), height.to_string()));
    }
    if let Some(decorations) = window.decorations {
        envs.push((
            "TELAR_DEV_WINDOW_DECORATIONS".to_string(),
            if decorations { "1" } else { "0" }.to_string(),
        ));
    }
    if let Some(resizable) = window.resizable {
        envs.push((
            "TELAR_DEV_WINDOW_RESIZABLE".to_string(),
            if resizable { "1" } else { "0" }.to_string(),
        ));
    }
    if let Some(transparent) = window.transparent {
        envs.push((
            "TELAR_DEV_WINDOW_TRANSPARENT".to_string(),
            if transparent { "1" } else { "0" }.to_string(),
        ));
    }
    if let Some(fullscreen) = &window.fullscreen {
        envs.push((
            "TELAR_DEV_WINDOW_FULLSCREEN".to_string(),
            fullscreen.clone(),
        ));
    }
    if let Some(position) = &window.position {
        envs.push(("TELAR_DEV_WINDOW_POSITION".to_string(), position.clone()));
    }
}

fn is_asset_extension(ext: &str) -> bool {
    ASSET_KINDS
        .iter()
        .any(|kind| kind.extensions.contains(&ext))
}

/// Everything an edit that should rebuild can come from.
struct WatchSet {
    /// Watched recursively: each member's `src/`, asset root and catalog directory.
    dirs: Vec<PathBuf>,
    /// Every `telar.toml` a member's settings are read from, present or not. A `prelude` or `theme` changes the Rust every `.rsx` becomes, so an edit there is a source edit.
    manifests: Vec<PathBuf>,
}

impl WatchSet {
    /// Canonical, because an event names the path the platform resolved — macOS reports `/private/var/…` for a watch on `/var/…` — and a set spelled any other way would match nothing.
    fn collect(workspace_root: &Path) -> Self {
        let canonical = |path: PathBuf| path.canonicalize().unwrap_or(path);
        Self {
            dirs: collect_watch_dirs(workspace_root)
                .into_iter()
                .map(canonical)
                .collect(),
            manifests: collect_watch_manifests(workspace_root)
                .into_iter()
                .map(|manifest| match (manifest.parent(), manifest.file_name()) {
                    (Some(parent), Some(name)) => canonical(parent.to_path_buf()).join(name),
                    _ => manifest,
                })
                .collect(),
        }
    }

    /// Whether the event should trigger a rebuild. Assets need no special handling any more: the macro emits an `include_bytes!` per baked asset, so cargo sees the edit as a real dependency, and the bake before each rebuild refreshes the artifact it reads.
    ///
    /// A manifest's directory is watched only for the manifest, so anything else changing beside it — a `Cargo.lock` cargo rewrote, an editor's swap file — is not an edit to the project.
    fn wants(&self, event: &notify::Event) -> bool {
        if !matches!(
            event.kind,
            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
        ) {
            return false;
        }
        event.paths.iter().any(|p| {
            if self.manifests.contains(p) {
                return true;
            }
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            self.dirs.iter().any(|dir| p.starts_with(dir))
                && (matches!(ext, "rs" | "rsx" | "toml") || is_asset_extension(ext))
        })
    }

    /// The directories to hand the watcher, each with how deep to watch it. A manifest is watched through its directory rather than as a file, because an editor that saves by writing a new file and renaming it over the old one would end a watch on the file itself; and not at all when a recursive watch already covers it.
    fn watches(&self) -> Vec<(PathBuf, RecursiveMode)> {
        let mut parents: Vec<PathBuf> = self
            .manifests
            .iter()
            .filter_map(|manifest| manifest.parent().map(Path::to_path_buf))
            .filter(|parent| parent.is_dir())
            .filter(|parent| !self.dirs.iter().any(|dir| parent.starts_with(dir)))
            .collect();
        parents.sort();
        parents.dedup();
        self.dirs
            .iter()
            .map(|dir| (dir.clone(), RecursiveMode::Recursive))
            .chain(
                parents
                    .into_iter()
                    .map(|parent| (parent, RecursiveMode::NonRecursive)),
            )
            .collect()
    }
}

/// The `telar.toml` of every member and of the workspace root — the files [`telar_project::TelarManifest::files`] reads for any member — whether or not each exists yet, so creating one is noticed too.
fn collect_watch_manifests(workspace_root: &Path) -> Vec<PathBuf> {
    let mut manifests: Vec<PathBuf> = super::bake::member_dirs(workspace_root)
        .into_iter()
        .chain(std::iter::once(workspace_root.to_path_buf()))
        .map(|dir| dir.join(telar_project::MANIFEST_FILENAME))
        .collect();
    manifests.sort();
    manifests.dedup();
    manifests
}

// Every directory an edit can come from: each member's `src/`, the asset root, and the catalog directory. The last two sit outside `src/` by default, so watching only `src/` meant editing an asset or a translation raised no event at all — not one that was handled badly, one that never arrived.
fn collect_watch_dirs(workspace_root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = super::bake::member_dirs(workspace_root)
        .into_iter()
        .flat_map(|member| {
            [
                Some(member.join("src")),
                Some(telar_project::assets_root(&member)),
                telar_baker::locales_root(&member),
                // The application's own icons, not its Iconify sets: those usually live in `node_modules`, which is no tree to watch recursively.
                telar_project::TelarManifest::load_or_default(&member)
                    .telar
                    .icons
                    .and_then(|icons| icons.svg_dir(&member)),
            ]
        })
        .flatten()
        .filter(|dir| dir.is_dir())
        .collect();
    dirs.sort();
    dirs.dedup();
    // Each watch is recursive, and `[telar] assets` may point inside `src/` — watching both would deliver every edit twice and rebuild twice for one keystroke.
    let nested: Vec<PathBuf> = dirs
        .iter()
        .filter(|dir| {
            dirs.iter()
                .any(|other| *dir != other && dir.starts_with(other))
        })
        .cloned()
        .collect();
    dirs.retain(|dir| !nested.contains(dir));
    dirs
}

// TCP loopback rather than a unix socket, so hot reload works on non-Unix hosts. cargo-telar binds and the app connects once at startup, then reads line events.
struct HotChannel {
    listener: std::net::TcpListener,
    stream: Option<std::net::TcpStream>,
    port: u16,
}

impl HotChannel {
    fn bind() -> Self {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("[cargo-telar] failed to bind hot reload port");
        let port = listener
            .local_addr()
            .expect("[cargo-telar] failed to read hot reload port")
            .port();
        // Non-blocking so send() can drain pending connections without stalling the watch loop.
        listener
            .set_nonblocking(true)
            .expect("[cargo-telar] failed to configure hot reload listener");
        HotChannel {
            listener,
            stream: None,
            port,
        }
    }

    fn notify_hot_reload(&mut self, lib_path: &str) {
        self.send(&format!("{HOT_RELOAD_PREFIX}{lib_path}"));
    }

    fn notify_build_error(&mut self, message: &str) {
        // Escaped rather than replaced: a code frame is full of `|`, so a ` | ` separator would be cut apart at every gutter. Backslashes go first, or an escape in the message decodes as a line break.
        let escaped = message
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\r', "");
        self.send(&format!("{BUILD_ERROR_PREFIX}{escaped}"));
    }

    /// Answers whether the line reached the app.
    fn send(&mut self, message: &str) -> bool {
        use std::io::Write;
        // The app's connection sits in the accept backlog until the first send; a reconnect replaces the previous stream.
        while let Ok((stream, _)) = self.listener.accept() {
            self.stream = Some(stream);
        }
        match &mut self.stream {
            Some(stream) => match writeln!(stream, "{message}") {
                Ok(()) => true,
                Err(e) => {
                    eprintln!("[cargo-telar] Failed to write to hot reload channel: {e}");
                    self.stream = None;
                    false
                }
            },
            None => {
                eprintln!("[cargo-telar] App not connected to the hot reload channel.");
                false
            }
        }
    }
}

fn make_watcher(
    tx: mpsc::Sender<notify::Result<notify::Event>>,
    watched: &WatchSet,
) -> RecommendedWatcher {
    let mut watcher = RecommendedWatcher::new(tx, NotifyConfig::default())
        .expect("[cargo-telar] failed to create file watcher");
    for (dir, mode) in watched.watches() {
        watcher.watch(&dir, mode).unwrap_or_else(|e| {
            eprintln!(
                "[cargo-telar] warning: could not watch {}: {e}",
                dir.display()
            )
        });
    }
    watcher
}

/// The app the hot-reload loop runs: the binary and what it is started with.
struct HotLaunch {
    bin_path: PathBuf,
    lib_path: PathBuf,
    args: Vec<String>,
    envs: Vec<(String, String)>,
}

fn watch_and_hot_reload(
    build_args: Vec<String>,
    launch: HotLaunch,
    mut channel: HotChannel,
    workshop: Option<WorkshopChannel>,
    workspace_root: PathBuf,
) -> ! {
    let HotLaunch {
        bin_path,
        lib_path,
        args,
        envs,
    } = launch;
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let watched = WatchSet::collect(&workspace_root);
    let _watcher = make_watcher(tx, &watched);

    eprintln!("[cargo-telar] Starting with hot reload...");
    let mut child = Command::new(&bin_path)
        .args(&args)
        .env("TELAR_HOT_LIB", lib_path.to_str().unwrap_or_default())
        .env("TELAR_HOT_PORT", channel.port.to_string())
        .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .spawn()
        .expect("[cargo-telar] failed to spawn app binary");

    let debounce = Duration::from_millis(200);
    let mut last_event = Instant::now();
    let mut pending_rebuild = false;

    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                eprintln!("[cargo-telar] App exited.");
                // `exit` runs no destructors, and the channel's removes the file naming a workshop that is gone.
                drop(workshop);
                std::process::exit(0);
            }
            Ok(None) => {}
            Err(e) => eprintln!("[cargo-telar] error: {e}"),
        }

        if let Some(workshop) = &workshop {
            workshop.poll(|line| channel.send(line));
        }

        while let Ok(Ok(event)) = rx.try_recv() {
            if watched.wants(&event) {
                last_event = Instant::now();
                pending_rebuild = true;
            }
        }

        if pending_rebuild && last_event.elapsed() >= debounce {
            pending_rebuild = false;
            while rx.try_recv().is_ok() {}
            eprintln!("[cargo-telar] Change detected, rebuilding...");
            // `super::run` prepares once, before the loop starts. Every rebuild after that invokes cargo directly, so without this an edited asset compiles against the artifact from startup — and since the macro checks its hash, that is a build failure rather than a stale drawing.
            super::bake::bake_workspace();
            super::transpile::transpile_workspace();
            let mut cmd = Command::new("cargo");
            cmd.args(&build_args)
                .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            let (succeeded, report) = build_with_diagnostics(&mut cmd);
            if !report.is_empty() {
                eprintln!();
                eprint!("{}", report.render(true));
            }
            if succeeded {
                channel.notify_hot_reload(lib_path.to_str().unwrap_or_default());
                eprintln!("[cargo-telar] Hot reloaded.");
            } else {
                eprintln!("[cargo-telar] Build failed, waiting for changes...");
                channel.notify_build_error(&report.render(false));
            }
        }

        if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(50))
            && watched.wants(&event)
        {
            last_event = Instant::now();
            pending_rebuild = true;
        }
    }
}

fn watch_and_run(
    cargo_args: Vec<String>,
    envs: Vec<(String, String)>,
    workspace_root: PathBuf,
) -> ! {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let watched = WatchSet::collect(&workspace_root);
    let _watcher = make_watcher(tx, &watched);

    loop {
        eprintln!("[cargo-telar] Starting...");
        super::bake::bake_workspace();
        super::transpile::transpile_workspace();
        let mut child = Command::new("cargo")
            .args(&cargo_args)
            .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .spawn()
            .expect("[cargo-telar] failed to spawn cargo");

        let debounce = Duration::from_millis(200);
        let mut last_event = Instant::now();
        let mut pending_restart = false;

        'watch: loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Killed by signal (e.g. Ctrl+C) — propagate and exit
                    if status.code().is_none() {
                        std::process::exit(130);
                    }
                    let code = status.code().unwrap_or(1);
                    if code == 0 {
                        std::process::exit(0);
                    }
                    eprintln!("[cargo-telar] Process exited ({code}). Watching for changes...");
                    loop {
                        match rx.recv() {
                            Ok(Ok(event)) if watched.wants(&event) => {
                                while rx.try_recv().is_ok() {}
                                eprintln!("[cargo-telar] Change detected, restarting...");
                                break 'watch;
                            }
                            _ => {}
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => eprintln!("[cargo-telar] error: {e}"),
            }

            while let Ok(Ok(event)) = rx.try_recv() {
                if watched.wants(&event) {
                    last_event = Instant::now();
                    pending_restart = true;
                }
            }

            if pending_restart && last_event.elapsed() >= debounce {
                while rx.try_recv().is_ok() {}
                eprintln!("[cargo-telar] Change detected, restarting...");
                child.kill().ok();
                child.wait().ok();
                break 'watch;
            }

            if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(50))
                && watched.wants(&event)
            {
                last_event = Instant::now();
                pending_restart = true;
            }
        }
    }
}

pub(crate) enum HotMode {
    Dev,
    Preview,
}

impl HotMode {
    fn is_preview(&self) -> bool {
        matches!(self, HotMode::Preview)
    }

    /// What the loop builds with, on top of the frontend `select_frontend` already named.
    ///
    /// No window among them: `telar/dev` used to carry `desktop-bare` so that a project which declared no frontend still got one, and that window then rode along into `--target tui` — a terminal session linking winit for a frame it never draws. The frontend is the target's business now, and a build that names none reaches the same "no frontend to run on" panic `cargo telar build` has always produced.
    fn features(&self) -> &'static [&'static str] {
        match self {
            HotMode::Dev => &["telar/dev"],
            // `telar/preview` names `desktop-bare` itself: the preview host is a window by definition, and a `--target tui` preview still goes through it to reach the entry point that renders the blocks.
            HotMode::Preview => &["telar/preview", "telar/dev"],
        }
    }

    /// What the dylib half of the loop adds. Named on the build rather than pushed through `RUSTFLAGS` as a `--cfg`: rustflags are hashed into every unit in the graph, so the old spelling recompiled all four hundred dependencies on each switch between this loop and `cargo telar check`, to change three crates. Cargo tracks a feature just as well and scopes it to the crates that enable it.
    fn hot_features(&self) -> &'static [&'static str] {
        match self {
            HotMode::Dev => &["telar/dev", "telar/hot-reload"],
            HotMode::Preview => &["telar/preview", "telar/dev", "telar/hot-reload"],
        }
    }
}

/// The package's own feature for the devtools overlay, qualified so it reaches the package from a workspace root too. `None` when the session turned the overlay off, or when the package declares no `telar-devtools` to turn on, which is said with the command that adds it.
fn devtools_feature(resolved: &ResolvedPackage, config: &TelarSection) -> Option<String> {
    if config.dev.devtools == Some(false) {
        return None;
    }
    let package = resolved.name();
    if !telar_project::declares_optional_dependency(&resolved.package_dir, DEVTOOLS_PACKAGE) {
        eprintln!("[cargo-telar] {}", missing_devtools_note(&package));
        return None;
    }
    Some(format!("{package}/{DEVTOOLS_PACKAGE}"))
}

/// The package's own feature for the workshop `cargo telar preview` shows the previews in. `None` in a dev session, and when the package declares no `telar-workshop`, which is said with the command that adds it: the previews then open in the plain page.
fn workshop_feature(mode: &HotMode, resolved: &ResolvedPackage) -> Option<String> {
    if !mode.is_preview() {
        return None;
    }
    let package = resolved.name();
    if !telar_project::declares_optional_dependency(&resolved.package_dir, WORKSHOP_PACKAGE) {
        eprintln!("[cargo-telar] {}", missing_workshop_note(&package));
        return None;
    }
    Some(format!("{package}/{WORKSHOP_PACKAGE}"))
}

fn with_tooling<'a>(features: &[&'a str], tooling: &[&'a str]) -> Vec<&'a str> {
    features.iter().chain(tooling).copied().collect()
}

pub(crate) struct HotLoopOpts {
    pub(crate) args: Vec<String>,
    /// What the app binary itself is started with, after cargo's own arguments.
    pub(crate) app_args: Vec<String>,
    pub(crate) config: TelarSection,
    pub(crate) no_hot_reload: bool,
}

/// `cargo run`'s arguments with `app_args` handed on to the binary.
fn with_app_args(mut cargo_args: Vec<String>, app_args: &[String]) -> Vec<String> {
    if app_args.is_empty() {
        return cargo_args;
    }
    if !cargo_args.iter().any(|arg| arg == "--") {
        cargo_args.push("--".to_string());
    }
    cargo_args.extend(app_args.iter().cloned());
    cargo_args
}

pub(crate) fn run_hot_loop(mode: HotMode, opts: HotLoopOpts) -> ! {
    let HotLoopOpts {
        args,
        app_args,
        config,
        no_hot_reload,
    } = opts;

    let (android, rest) = split_android_flag(args);
    let resolved = resolve_package(&rest);
    let tooling_features: Vec<String> = devtools_feature(&resolved, &config)
        .into_iter()
        .chain(workshop_feature(&mode, &resolved))
        .collect();
    let tooling: Vec<&str> = tooling_features.iter().map(String::as_str).collect();
    let features = with_tooling(mode.features(), &tooling);
    let backend_value = backend_as_str(config.backend.unwrap_or_default());
    let is_preview = mode.is_preview();

    if android {
        warn_if_tooling_unlocked(&rest, mode.features());
        // `cargo apk run --lib` crashes on UID parsing when launching; work around by doing build → adb install → adb shell am start manually.
        let mut build_args = vec!["apk".to_string(), "build".to_string(), "--lib".to_string()];
        build_args.extend(rest.iter().cloned());
        for feature in &features {
            inject_feature(&mut build_args, feature);
        }

        let status = make_android_cmd(build_args, config)
            .status()
            .expect("[cargo-telar] failed to invoke cargo");
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }

        android_install_and_launch(&rest);
        std::process::exit(0);
    }

    let mut launch_envs = vec![(
        "TELAR_RENDERER_BACKEND".to_string(),
        backend_value.to_string(),
    )];
    if is_preview {
        launch_envs.push(("TELAR_PREVIEW".to_string(), "1".to_string()));
        launch_envs.push((
            WORKSPACE_DIR_VAR.to_string(),
            resolved.workspace_root.display().to_string(),
        ));
    }
    if config.dev.devtools == Some(false) {
        launch_envs.push(("TELAR_DEVTOOLS".to_string(), "0".to_string()));
    }
    if let Some(window) = &config.dev.window {
        apply_dev_window_env(&mut launch_envs, window);
    }

    let mut cargo_args = vec!["run".to_string()];
    cargo_args.extend(rest.clone());
    for feature in &features {
        inject_feature(&mut cargo_args, feature);
    }

    let workspace_root = resolved.workspace_root.clone();
    let profile = profile_of(&rest);

    // Gated on the manifest rather than the built artifact: the dylib build differs in both RUSTFLAGS and generated sources, so running it for a package that can never produce one compiles the crate graph twice per `cargo telar dev` and leaves each half stale.
    let hot_reload = !no_hot_reload && resolved.produces_cdylib;
    if !no_hot_reload && !hot_reload {
        eprintln!(
            "[cargo-telar] Hot reload off: `{}` declares no `[lib] crate-type = [\"cdylib\", ..]`. Restarting the process on change instead.",
            resolved.name()
        );
    }
    warn_if_tooling_unlocked(
        &rest,
        if hot_reload {
            mode.hot_features()
        } else {
            mode.features()
        },
    );

    if hot_reload {
        let hot_features = with_tooling(mode.hot_features(), &tooling);
        let package_name = resolved.name();
        let lib_path = package_lib_path(&workspace_root, &package_name, profile);
        let bin_path = package_bin_path(&workspace_root, &package_name, profile);

        let mut build_args = vec!["build".to_string()];
        build_args.extend(rest.clone());
        for feature in &hot_features {
            inject_feature(&mut build_args, feature);
        }
        with_json_messages(&mut build_args);
        eprintln!("[cargo-telar] Building...");
        let mut build_cmd = Command::new("cargo");
        build_cmd
            .args(&build_args)
            // `telar`'s backend is resolved with `option_env!` and so is a tracked build input: omitting it would compile a different backend than the hot rebuilds.
            .env("TELAR_RENDERER_BACKEND", backend_value);
        let (succeeded, report) = build_with_diagnostics(&mut build_cmd);
        if !report.is_empty() {
            eprintln!();
            eprint!("{}", report.render(true));
        }
        if !succeeded {
            eprintln!("[cargo-telar] Initial build failed. Watching for changes...");
        }

        if bin_path.exists() && lib_path.exists() {
            let lib_build_args = make_lib_build_args(&rest, &hot_features);
            let workshop = is_preview
                .then(|| WorkshopChannel::open(channel_file(&workspace_root, &package_name)))
                .flatten();

            watch_and_hot_reload(
                lib_build_args,
                HotLaunch {
                    bin_path,
                    lib_path,
                    args: app_args,
                    envs: launch_envs,
                },
                HotChannel::bind(),
                workshop,
                workspace_root,
            );
        }
    }

    watch_and_run(
        with_app_args(cargo_args, &app_args),
        launch_envs,
        workspace_root,
    );
}

#[cfg(test)]
#[path = "watch_test.rs"]
mod tests;
