//! `cargo telar dev`: the watch loop, the rebuild, and the hot-reload channel to the running app.

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use telar_transpiler::ASSET_KINDS;

use super::android::{android_install_and_launch, make_android_cmd};
use super::config::{
    TelarConfig, WindowConfig, backend_as_str, resolve_package, split_android_flag,
};
use super::diagnostics;
use super::package::{package_bin_path, package_lib_path, profile_of};

fn inject_feature(args: &mut Vec<String>, feature: &str) {
    if let Some(pos) = args.iter().position(|a| a == "--features" || a == "-F") {
        if pos + 1 < args.len() {
            args[pos + 1] = format!("{},{feature}", args[pos + 1]);
            return;
        }
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

fn make_lib_build_args(args: &[String], features: &[&str]) -> Vec<String> {
    let mut lib_build_args = vec!["build".to_string(), "--lib".to_string()];
    for pair in args.windows(2) {
        if pair[0] == "-p" || pair[0] == "--package" {
            lib_build_args.push(pair[0].clone());
            lib_build_args.push(pair[1].clone());
        }
    }
    if args.contains(&"--release".to_string()) {
        lib_build_args.push("--release".to_string());
    }
    for feature in features {
        inject_feature(&mut lib_build_args, feature);
    }
    with_json_messages(&mut lib_build_args);
    lib_build_args
}

fn apply_dev_window_env(envs: &mut Vec<(String, String)>, window: &WindowConfig) {
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

// Whether the event should trigger a rebuild. Assets need no special handling any more: the macro emits an `include_bytes!` per baked asset, so cargo sees the edit as a real dependency, and the bake before each rebuild refreshes the artifact it reads.
fn note_event(event: &notify::Event) -> bool {
    if !matches!(
        event.kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
    ) {
        return false;
    }
    event.paths.iter().any(|p| {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        matches!(ext, "rs" | "rsx" | "toml") || is_asset_extension(ext)
    })
}

// Every directory an edit can come from: each member's `src/`, the asset root, and the catalog directory. The last two sit outside `src/` by default, so watching only `src/` meant editing an asset or a translation raised no event at all — not one that was handled badly, one that never arrived.
fn collect_watch_dirs(workspace_root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = super::bake::member_dirs(workspace_root)
        .into_iter()
        .flat_map(|member| {
            [
                Some(member.join("src")),
                Some(telar_transpiler::assets_root(&member)),
                telar_baker::locales_root(&member),
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
        self.send(&format!("hot:{lib_path}"));
    }

    fn notify_build_error(&mut self, message: &str) {
        // Escaped rather than replaced: a code frame is full of `|`, so a ` | ` separator would be cut apart at every gutter. Backslashes go first, or an escape in the message decodes as a line break.
        let escaped = message
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\r', "");
        self.send(&format!("err:{escaped}"));
    }

    fn send(&mut self, message: &str) {
        use std::io::Write;
        // The app's connection sits in the accept backlog until the first send; a reconnect replaces the previous stream.
        while let Ok((stream, _)) = self.listener.accept() {
            self.stream = Some(stream);
        }
        match &mut self.stream {
            Some(stream) => {
                if let Err(e) = writeln!(stream, "{message}") {
                    eprintln!("[cargo-telar] Failed to write to hot reload channel: {e}");
                    self.stream = None;
                }
            }
            None => eprintln!("[cargo-telar] App not connected to the hot reload channel."),
        }
    }
}

fn make_watcher(
    tx: mpsc::Sender<notify::Result<notify::Event>>,
    workspace_root: &Path,
) -> RecommendedWatcher {
    let mut watcher = RecommendedWatcher::new(tx, NotifyConfig::default())
        .expect("[cargo-telar] failed to create file watcher");
    for src_dir in collect_watch_dirs(workspace_root) {
        watcher
            .watch(&src_dir, RecursiveMode::Recursive)
            .unwrap_or_else(|e| {
                eprintln!(
                    "[cargo-telar] warning: could not watch {}: {e}",
                    src_dir.display()
                )
            });
    }
    watcher
}

fn watch_and_hot_reload(
    build_args: Vec<String>,
    bin_path: PathBuf,
    lib_path: PathBuf,
    mut channel: HotChannel,
    envs: Vec<(String, String)>,
    workspace_root: PathBuf,
) -> ! {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let _watcher = make_watcher(tx, &workspace_root);

    eprintln!("[cargo-telar] Starting with hot reload...");
    let mut child = Command::new(&bin_path)
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
                std::process::exit(0);
            }
            Ok(None) => {}
            Err(e) => eprintln!("[cargo-telar] error: {e}"),
        }

        while let Ok(Ok(event)) = rx.try_recv() {
            if note_event(&event) {
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

        if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(50)) {
            if note_event(&event) {
                last_event = Instant::now();
                pending_rebuild = true;
            }
        }
    }
}

fn watch_and_run(
    cargo_args: Vec<String>,
    envs: Vec<(String, String)>,
    workspace_root: PathBuf,
) -> ! {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let _watcher = make_watcher(tx, &workspace_root);

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
                            Ok(Ok(event)) if note_event(&event) => {
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
                if note_event(&event) {
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

            if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(50)) {
                if note_event(&event) {
                    last_event = Instant::now();
                    pending_restart = true;
                }
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

    fn features(&self) -> &'static [&'static str] {
        match self {
            HotMode::Dev => &["telar/dev"],
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

pub(crate) struct HotLoopOpts {
    pub(crate) args: Vec<String>,
    pub(crate) config: TelarConfig,
    pub(crate) no_hot_reload: bool,
}

pub(crate) fn run_hot_loop(mode: HotMode, opts: HotLoopOpts) -> ! {
    let HotLoopOpts {
        args,
        config,
        no_hot_reload,
    } = opts;

    let (android, rest) = split_android_flag(args);
    let features = mode.features();
    let backend_value = backend_as_str(config.backend.unwrap_or_default());
    let is_preview = mode.is_preview();

    if android {
        // `cargo apk run --lib` crashes on UID parsing when launching; work around by doing build → adb install → adb shell am start manually.
        let mut build_args = vec!["apk".to_string(), "build".to_string(), "--lib".to_string()];
        build_args.extend(rest.iter().cloned());
        for feature in features {
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
    }
    let devtools_disabled = config.dev.as_ref().and_then(|d| d.devtools) == Some(false);
    if devtools_disabled {
        launch_envs.push(("TELAR_DEVTOOLS".to_string(), "0".to_string()));
    }
    if let Some(dev) = &config.dev
        && let Some(window) = &dev.window
    {
        apply_dev_window_env(&mut launch_envs, window);
    }

    let mut cargo_args = vec!["run".to_string()];
    cargo_args.extend(rest.clone());
    for feature in features {
        inject_feature(&mut cargo_args, feature);
    }

    let resolved = resolve_package(&rest);
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

    if hot_reload {
        let hot_features = mode.hot_features();
        let package_name = resolved.name();
        let lib_path = package_lib_path(&workspace_root, &package_name, profile);
        let bin_path = package_bin_path(&workspace_root, &package_name, profile);

        let mut build_args = vec!["build".to_string()];
        build_args.extend(rest.clone());
        for feature in hot_features {
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
            let lib_build_args = make_lib_build_args(&rest, hot_features);

            watch_and_hot_reload(
                lib_build_args,
                bin_path,
                lib_path,
                HotChannel::bind(),
                launch_envs,
                workspace_root,
            );
        }
    }

    watch_and_run(cargo_args, launch_envs, workspace_root);
}

#[cfg(test)]
#[path = "watch_test.rs"]
mod tests;
