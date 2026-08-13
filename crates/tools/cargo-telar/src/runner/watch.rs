use std::fs::OpenOptions;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use super::android::{android_install_and_launch, make_android_cmd};
use super::config::{
    CargoManifest, TelarConfig, WindowConfig, backend_as_str, expand_member, resolve_package,
    split_android_flag,
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
/// stdout carries the JSON diagnostic stream and is consumed here; stderr is cargo's own progress and is
/// left on the terminal, so a build still looks like a build. Every build goes through this, not only the
/// failing ones — warnings used to be captured into a `String` that was read on the failure path alone,
/// which made them invisible for a whole development session.
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

/// Adds `--message-format=json` unless the caller already chose a format, which cargo would reject as two
/// of them rather than honour the later one.
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

fn is_source_event(event: &notify::Event) -> bool {
    if !matches!(
        event.kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
    ) {
        return false;
    }
    event.paths.iter().any(|p| {
        matches!(
            p.extension().and_then(|e| e.to_str()).unwrap_or(""),
            "rs" | "rsx" | "toml" | "svg" | "png" | "jpg" | "jpeg"
        )
    })
}

// An asset change (svg/png/jpg/jpeg) leaves every `.rsx` untouched, so cargo's fingerprint is unchanged and the baker never re-runs; these events need the `.rsx`-touch workaround below.
fn is_asset_event(event: &notify::Event) -> bool {
    if !matches!(
        event.kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
    ) {
        return false;
    }
    event.paths.iter().any(|p| {
        matches!(
            p.extension().and_then(|e| e.to_str()).unwrap_or(""),
            "svg" | "png" | "jpg" | "jpeg"
        )
    })
}

// Classifies an event for the watch loops: returns whether it should trigger a rebuild, and eagerly forces a re-bake when only an asset changed. Touching an asset's `.rsx` produces a `.rsx` event that is a source (not asset) event, so it never re-enters this touch path — no cascade.
fn note_event(event: &notify::Event, src_dirs: &[PathBuf]) -> bool {
    if !is_source_event(event) {
        return false;
    }
    if is_asset_event(event) {
        touch_rsx_files(src_dirs);
    }
    true
}

// Proc macros can't emit `cargo:rerun-if-changed` and read assets via `std::fs` (untracked by rustc), so an asset-only edit never re-runs the baker. Bumping the mtime of every watched `.rsx` (each wired into rustc's dep graph via `include_str!`) forces the recompile that re-bakes. Coarse for v1: it touches all `.rsx`, not just the one referencing the asset; a precise asset->`.rsx` manifest is a future refinement.
fn touch_rsx_files(src_dirs: &[PathBuf]) {
    let now = SystemTime::now();
    for dir in src_dirs {
        touch_rsx_in_dir(dir, now);
    }
}

// Recursive `.rsx` walk, parallel to telar-transpiler's find_rsx_files discovery walk but touching mtimes rather than collecting paths.
fn touch_rsx_in_dir(dir: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            touch_rsx_in_dir(&path, now);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rsx") {
            // write(true) opens without truncating; set_modified needs the file opened for writing.
            if let Ok(file) = OpenOptions::new().write(true).open(&path) {
                let _ = file.set_modified(now);
            }
        }
    }
}

fn collect_src_dirs(workspace_root: &Path) -> Vec<PathBuf> {
    let manifest_path = workspace_root.join("Cargo.toml");
    let Ok(content) = std::fs::read_to_string(&manifest_path) else {
        return vec![];
    };
    let Ok(manifest) = toml::from_str::<CargoManifest>(&content) else {
        return vec![];
    };
    let members = manifest.workspace.map(|w| w.members).unwrap_or_default();
    members
        .iter()
        .flat_map(|pattern| expand_member(workspace_root, pattern))
        .map(|member| member.join("src"))
        .filter(|src| src.is_dir())
        .collect()
}

/// The host triple's `CARGO_TARGET_<TRIPLE>_RUSTFLAGS`, which is where a direnv/flake shell usually puts a
/// linker choice (target-scoped rather than global, so a cross build keeps its own toolchain's linker).
fn host_target_rustflags() -> Option<String> {
    let output = Command::new("rustc").arg("-vV").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let host = text
        .lines()
        .find_map(|line| line.strip_prefix("host: "))?
        .trim();
    let key = format!(
        "CARGO_TARGET_{}_RUSTFLAGS",
        host.to_uppercase().replace('-', "_")
    );
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

/// The flags this loop must build with, on top of whatever the developer already configured.
///
/// Cargo reads rustflags from exactly one source and `RUSTFLAGS` outranks the rest, so setting it here
/// silently discards the target-scoped tier — which is where a Nix shell or a `.envrc` puts `-fuse-ld=mold`.
/// Folding that tier in when `RUSTFLAGS` is unset keeps the developer's choice instead of quietly undoing it.
fn hot_reload_rustflags() -> String {
    with_hot_reload_cfg(
        std::env::var("RUSTFLAGS")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(host_target_rustflags),
    )
}

fn with_hot_reload_cfg(inherited: Option<String>) -> String {
    // Adding --cfg=telar_hot_reload changes the Cargo fingerprint, forcing a recompile so the proc macro re-runs with TELAR_HOT_RELOAD_BUILD=1 and generates the hot reload code.
    let flag = "--cfg=telar_hot_reload";
    match inherited {
        Some(existing) => format!("{existing} {flag}"),
        None => flag.to_string(),
    }
}

fn preview_rustflags() -> String {
    // --cfg=telar_preview is in the fingerprint so Cargo always recompiles when switching between dev and preview, ensuring the proc-macro-generated _rsx_hot_create_app includes or omits the preview branch correctly.
    format!("{} --cfg=telar_preview", hot_reload_rustflags())
}

// TCP loopback channel to the running app (instead of a unix socket, so hot reload works on non-Unix hosts). cargo-telar binds, the app connects once at startup (TELAR_HOT_PORT) and reads line events.
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
        // Escaped rather than replaced: a code frame is full of `|`, so the ` | ` separator this used to substitute would be cut back apart at every gutter. Backslashes go first, or an escape in the message decodes as a line break.
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
    for src_dir in collect_src_dirs(workspace_root) {
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
    rustflags: String,
    workspace_root: PathBuf,
) -> ! {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let _watcher = make_watcher(tx, &workspace_root);
    let src_dirs = collect_src_dirs(&workspace_root);

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
            if note_event(&event, &src_dirs) {
                last_event = Instant::now();
                pending_rebuild = true;
            }
        }

        if pending_rebuild && last_event.elapsed() >= debounce {
            pending_rebuild = false;
            while rx.try_recv().is_ok() {}
            eprintln!("[cargo-telar] Change detected, rebuilding...");
            let mut cmd = Command::new("cargo");
            cmd.args(&build_args)
                .env("TELAR_HOT_RELOAD_BUILD", "1")
                .env("RUSTFLAGS", &rustflags)
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
            if note_event(&event, &src_dirs) {
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
    let src_dirs = collect_src_dirs(&workspace_root);

    loop {
        eprintln!("[cargo-telar] Starting...");
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
                            Ok(Ok(event)) if note_event(&event, &src_dirs) => {
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
                if note_event(&event, &src_dirs) {
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
                if note_event(&event, &src_dirs) {
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
            HotMode::Preview => &["rsx/preview", "telar/dev"],
        }
    }

    fn rustflags(&self) -> String {
        match self {
            HotMode::Dev => hot_reload_rustflags(),
            HotMode::Preview => preview_rustflags(),
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

    // Gated on the manifest rather than on the built artifact: the dylib build differs from the plain one in both RUSTFLAGS and the generated sources, so running it for a package that can never produce a dylib compiles the crate graph twice per `cargo telar dev` and leaves each half stale for the next run.
    let hot_reload = !no_hot_reload && resolved.produces_cdylib;
    if !no_hot_reload && !hot_reload {
        eprintln!(
            "[cargo-telar] Hot reload off: `{}` declares no `[lib] crate-type = [\"cdylib\", ..]`. Restarting the process on change instead.",
            resolved.name()
        );
    }

    if hot_reload {
        let rustflags = mode.rustflags();
        let package_name = resolved.name();
        let lib_path = package_lib_path(&workspace_root, &package_name, profile);
        let bin_path = package_bin_path(&workspace_root, &package_name, profile);

        // Initial build (produces both binary and dylib).
        let mut build_args = vec!["build".to_string()];
        build_args.extend(rest.clone());
        for feature in features {
            inject_feature(&mut build_args, feature);
        }
        with_json_messages(&mut build_args);
        eprintln!("[cargo-telar] Building...");
        let mut build_cmd = Command::new("cargo");
        build_cmd
            .args(&build_args)
            .env("TELAR_HOT_RELOAD_BUILD", "1")
            // `telar`'s backend is resolved with `option_env!`, so it is a tracked build input: omitting it here would compile a different backend than the hot rebuilds and make the first of them recompile the graph.
            .env("TELAR_RENDERER_BACKEND", backend_value)
            .env("RUSTFLAGS", &rustflags);
        if is_preview {
            build_cmd.env("TELAR_PREVIEW_BUILD", "1");
        }
        let (succeeded, report) = build_with_diagnostics(&mut build_cmd);
        if !report.is_empty() {
            eprintln!();
            eprint!("{}", report.render(true));
        }
        if !succeeded {
            eprintln!("[cargo-telar] Initial build failed. Watching for changes...");
        }

        if bin_path.exists() && lib_path.exists() {
            let lib_build_args = make_lib_build_args(&rest, features);

            let mut build_envs = launch_envs.clone();
            if is_preview {
                build_envs.push(("TELAR_PREVIEW_BUILD".to_string(), "1".to_string()));
            }

            watch_and_hot_reload(
                lib_build_args,
                bin_path,
                lib_path,
                HotChannel::bind(),
                build_envs,
                rustflags,
                workspace_root,
            );
        }
        // Fallback if binary or lib not found: use process-restart watch.
    }

    watch_and_run(cargo_args, launch_envs, workspace_root);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The linker a Nix or direnv shell configures for the host target used to be dropped on the floor:
    /// setting `RUSTFLAGS` here made Cargo stop reading the tier that carried it, so the one loop a developer
    /// tunes their linker *for* was the one loop that ignored it. Choosing a linker is not this tool's call,
    /// but discarding the choice already made is a bug.
    #[test]
    fn a_shell_configured_linker_survives_into_the_hot_reload_build() {
        assert_eq!(
            with_hot_reload_cfg(Some("-C link-arg=-fuse-ld=mold".to_string())),
            "-C link-arg=-fuse-ld=mold --cfg=telar_hot_reload"
        );
    }

    /// With nothing to inherit the cfg stands alone, so a bare shell does not get a leading space that would
    /// read as an empty flag.
    #[test]
    fn nothing_to_inherit_leaves_the_cfg_alone() {
        assert_eq!(with_hot_reload_cfg(None), "--cfg=telar_hot_reload");
    }
}
