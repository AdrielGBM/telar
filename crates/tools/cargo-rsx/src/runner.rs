use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand, ValueEnum};
use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rsx::RendererBackend;
use serde::Deserialize;

#[derive(Parser)]
#[command(
    name = "cargo-rsx",
    bin_name = "cargo rsx",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<RsxCommand>,
}

#[derive(Subcommand)]
enum RsxCommand {
    /// Start the app with hot reload (default)
    Dev(DevArgs),
    /// Show all component previews with hot reload
    Preview(PreviewArgs),
    /// Build the app for distribution
    Build(BuildArgs),
    /// Render every preview component headlessly and report failures
    Test(TestArgs),
    /// Create a new RSX project (not yet implemented)
    New {
        /// Project name
        name: String,
    },
    /// Check the development environment
    Doctor,
}

#[derive(clap::Args)]
struct CommonArgs {
    /// Package to use
    #[arg(short = 'p', long)]
    package: Option<String>,
    /// Additional Cargo features
    #[arg(short = 'F', long, value_name = "FEATURES")]
    features: Option<String>,
    /// Target platform
    #[arg(long, value_enum, default_value = "desktop")]
    target: Target,
    /// Renderer backend
    #[arg(long, value_enum)]
    backend: Option<BackendArg>,
    /// Extra args passed directly to cargo (after --)
    #[arg(last = true)]
    cargo_args: Vec<String>,
}

/// Flags shared by the hot-reload commands (`dev` and `preview`).
#[derive(clap::Args)]
struct HotArgs {
    #[command(flatten)]
    common: CommonArgs,
    /// Build in release mode
    #[arg(long)]
    release: bool,
    /// Disable hot reload, restart process on changes instead
    #[arg(long)]
    no_hot_reload: bool,
}

#[derive(clap::Args)]
struct DevArgs {
    #[command(flatten)]
    hot: HotArgs,
    /// Devtools overlay
    #[arg(long, value_enum)]
    devtools: Option<DevtoolsArg>,
}

#[derive(clap::Args)]
struct PreviewArgs {
    #[command(flatten)]
    hot: HotArgs,
    /// Preview a specific component by name
    #[arg(long, conflicts_with = "list")]
    component: Option<String>,
    /// List all available previews and exit
    #[arg(long)]
    list: bool,
}

#[derive(clap::Args)]
struct TestArgs {
    #[command(flatten)]
    common: CommonArgs,
    /// Build in release mode
    #[arg(long)]
    release: bool,
}

#[derive(clap::Args)]
struct BuildArgs {
    #[command(flatten)]
    common: CommonArgs,
    /// Output package format
    #[arg(long, value_name = "FORMAT")]
    format: Option<BuildFormat>,
}

#[derive(Clone, ValueEnum)]
enum Target {
    Desktop,
    Android,
}

#[derive(Clone, ValueEnum)]
enum BackendArg {
    Auto,
    Hardware,
    Software,
}

#[derive(Clone, ValueEnum)]
enum DevtoolsArg {
    On,
    Off,
}

#[derive(Clone, ValueEnum)]
enum BuildFormat {
    Appimage,
    Deb,
    Dmg,
    Apk,
    Dir,
}

#[derive(Deserialize, Default, Clone)]
struct WindowConfig {
    pub title: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub decorations: Option<bool>,
    pub resizable: Option<bool>,
    pub transparent: Option<bool>,
    // "disabled" | "borderless" | "exclusive"
    pub fullscreen: Option<String>,
    // "centered" | "<x>,<y>"
    pub position: Option<String>,
}

#[derive(Deserialize, Default, Clone)]
struct DevConfig {
    #[serde(default)]
    pub window: Option<WindowConfig>,
    #[serde(default)]
    pub devtools: Option<bool>,
}

#[derive(Deserialize, Default)]
struct RsxConfig {
    #[serde(default)]
    pub backend: Option<RendererBackend>,
    #[serde(default)]
    pub dev: Option<DevConfig>,
}

#[derive(Deserialize, Default)]
struct RsxToml {
    #[serde(default)]
    pub rsx: RsxConfig,
}

#[derive(Deserialize, Default)]
struct CargoWorkspace {
    members: Vec<String>,
}
#[derive(Deserialize, Default)]
struct CargoManifest {
    workspace: Option<CargoWorkspace>,
    package: Option<CargoPackage>,
}
#[derive(Deserialize, Default)]
struct CargoPackage {
    name: String,
    metadata: Option<CargoPackageMetadata>,
}
#[derive(Deserialize, Default)]
struct CargoPackageMetadata {
    android: Option<AndroidMetadata>,
    // `[package.metadata.rsx]` — same schema as rsx.toml's `[rsx]`, but overridden by rsx.toml.
    rsx: Option<RsxConfig>,
}
#[derive(Deserialize, Default)]
struct AndroidMetadata {
    package: Option<String>,
}

fn expand_member(workspace_root: &Path, pattern: &str) -> Vec<PathBuf> {
    if let Some(prefix) = pattern.strip_suffix("/*") {
        std::fs::read_dir(workspace_root.join(prefix))
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect()
    } else {
        vec![workspace_root.join(pattern)]
    }
}

fn find_package_dir_in_workspace(workspace_root: &Path, package_name: &str) -> Option<PathBuf> {
    let workspace_manifest = std::fs::read_to_string(workspace_root.join("Cargo.toml")).ok()?;
    let manifest: CargoManifest = toml::from_str(&workspace_manifest).ok()?;
    let members = manifest.workspace?.members;

    for member_glob in members {
        for member_path in expand_member(workspace_root, &member_glob) {
            let cargo_toml = member_path.join("Cargo.toml");
            if let Ok(content) = std::fs::read_to_string(&cargo_toml)
                && let Ok(m) = toml::from_str::<CargoManifest>(&content)
                && m.package.map(|p| p.name == package_name).unwrap_or(false)
            {
                return Some(member_path);
            }
        }
    }
    None
}

fn find_package_dir(args: &[String]) -> PathBuf {
    let package_name = args
        .windows(2)
        .find(|pair| pair[0] == "-p" || pair[0] == "--package")
        .map(|pair| pair[1].as_str());

    if let Some(name) = package_name {
        let cwd = std::env::current_dir().unwrap_or_default();
        if let Some(root) = rsx_workspace::find_workspace_root(&cwd)
            && let Some(dir) = find_package_dir_in_workspace(&root, name)
        {
            return dir;
        }
    }

    let mut dir = std::env::current_dir().unwrap_or_default();
    loop {
        if dir.join("Cargo.toml").exists() {
            return dir;
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => return std::env::current_dir().unwrap_or_default(),
        }
    }
}

fn resolve_ndk_root() -> Option<String> {
    if let Ok(v) = std::env::var("ANDROID_NDK_ROOT") {
        if !v.is_empty() {
            return Some(v);
        }
    }
    let android_home = std::env::var("ANDROID_HOME").ok()?;
    let ndk_dir = Path::new(&android_home).join("ndk");
    let mut versions: Vec<_> = std::fs::read_dir(&ndk_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    versions.sort_by_key(|e| e.file_name());
    Some(versions.last()?.path().to_string_lossy().into_owned())
}

fn read_package_manifest(args: &[String]) -> Option<CargoPackage> {
    let dir = find_package_dir(args);
    let content = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    let manifest: CargoManifest = toml::from_str(&content).ok()?;
    manifest.package
}

fn android_package_id(args: &[String]) -> String {
    let pkg = read_package_manifest(args);
    if let Some(id) = pkg
        .as_ref()
        .and_then(|p| p.metadata.as_ref())
        .and_then(|m| m.android.as_ref())
        .and_then(|a| a.package.as_deref())
    {
        return id.to_owned();
    }
    let crate_name = pkg.map(|p| p.name).unwrap_or_else(|| "app".to_string());
    format!("com.example.{crate_name}")
}

fn apk_path(args: &[String]) -> PathBuf {
    let crate_dir = find_package_dir(args);
    let workspace_root =
        rsx_workspace::find_workspace_root(&crate_dir).unwrap_or(crate_dir.clone());
    let profile = if args.contains(&"--release".to_string()) {
        "release"
    } else {
        "debug"
    };
    let crate_name = read_package_manifest(args)
        .map(|p| p.name)
        .unwrap_or_else(|| "app".to_string());
    workspace_root
        .join("target")
        .join(profile)
        .join("apk")
        .join(format!("{crate_name}.apk"))
}

// Reads `[package.metadata.rsx]` from the package's Cargo.toml (the lowest-precedence file source).
fn read_manifest_config(dir: &Path) -> RsxConfig {
    let Ok(content) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
        return RsxConfig::default();
    };
    toml::from_str::<CargoManifest>(&content)
        .ok()
        .and_then(|m| m.package)
        .and_then(|p| p.metadata)
        .and_then(|m| m.rsx)
        .unwrap_or_default()
}

// Reads `[rsx]` from rsx.toml, which overrides the manifest metadata.
fn read_toml_config(dir: &Path) -> RsxConfig {
    let path = dir.join("rsx.toml");
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            toml::from_str::<RsxToml>(&content)
                .unwrap_or_else(|e| {
                    eprintln!(
                        "[cargo-rsx] Warning: failed to parse {}: {e}",
                        path.display()
                    );
                    RsxToml::default()
                })
                .rsx
        }
        Err(_) => RsxConfig::default(),
    }
}

// Config precedence, lowest to highest: built-in defaults < `[package.metadata.rsx]` (Cargo.toml) < `rsx.toml` < CLI flags. CLI flags are layered on by each command after this returns.
fn load_config(args: &[String]) -> RsxConfig {
    let dir = find_package_dir(args);
    merge_config(read_manifest_config(&dir), read_toml_config(&dir))
}

fn merge_opt<T>(base: Option<T>, over: Option<T>, merge: impl FnOnce(T, T) -> T) -> Option<T> {
    match (base, over) {
        (Some(b), Some(o)) => Some(merge(b, o)),
        (b, o) => o.or(b),
    }
}

fn merge_window(base: WindowConfig, over: WindowConfig) -> WindowConfig {
    WindowConfig {
        title: over.title.or(base.title),
        width: over.width.or(base.width),
        height: over.height.or(base.height),
        decorations: over.decorations.or(base.decorations),
        resizable: over.resizable.or(base.resizable),
        transparent: over.transparent.or(base.transparent),
        fullscreen: over.fullscreen.or(base.fullscreen),
        position: over.position.or(base.position),
    }
}

fn merge_dev(base: DevConfig, over: DevConfig) -> DevConfig {
    DevConfig {
        window: merge_opt(base.window, over.window, merge_window),
        devtools: over.devtools.or(base.devtools),
    }
}

fn merge_config(base: RsxConfig, over: RsxConfig) -> RsxConfig {
    RsxConfig {
        backend: over.backend.or(base.backend),
        dev: merge_opt(base.dev, over.dev, merge_dev),
    }
}

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
    lib_build_args
}

fn split_android_flag(args: Vec<String>) -> (bool, Vec<String>) {
    let android = args.contains(&"--android".to_string());
    let rest = args.into_iter().filter(|a| a != "--android").collect();
    (android, rest)
}

fn apply_dev_window_env(envs: &mut Vec<(String, String)>, window: &WindowConfig) {
    if let Some(title) = &window.title {
        envs.push(("RSX_DEV_WINDOW_TITLE".to_string(), title.clone()));
    }
    if let Some(width) = window.width {
        envs.push(("RSX_DEV_WINDOW_WIDTH".to_string(), width.to_string()));
    }
    if let Some(height) = window.height {
        envs.push(("RSX_DEV_WINDOW_HEIGHT".to_string(), height.to_string()));
    }
    if let Some(decorations) = window.decorations {
        envs.push((
            "RSX_DEV_WINDOW_DECORATIONS".to_string(),
            if decorations { "1" } else { "0" }.to_string(),
        ));
    }
    if let Some(resizable) = window.resizable {
        envs.push((
            "RSX_DEV_WINDOW_RESIZABLE".to_string(),
            if resizable { "1" } else { "0" }.to_string(),
        ));
    }
    if let Some(transparent) = window.transparent {
        envs.push((
            "RSX_DEV_WINDOW_TRANSPARENT".to_string(),
            if transparent { "1" } else { "0" }.to_string(),
        ));
    }
    if let Some(fullscreen) = &window.fullscreen {
        envs.push(("RSX_DEV_WINDOW_FULLSCREEN".to_string(), fullscreen.clone()));
    }
    if let Some(position) = &window.position {
        envs.push(("RSX_DEV_WINDOW_POSITION".to_string(), position.clone()));
    }
}

fn load_dotenv(cmd: &mut Command) {
    let cwd = std::env::current_dir().unwrap_or_default();
    let root = rsx_workspace::find_workspace_root(&cwd).unwrap_or(cwd);
    let path = root.join(".env");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            // Explicit env wins over .env: only set keys absent from the calling environment.
            if std::env::var(key).is_err() {
                // Resolve relative paths against the workspace root so values like "android-release.keystore" work regardless of the cwd at signing time.
                let resolved = root.join(value);
                let final_value = if resolved.exists() {
                    resolved.to_string_lossy().into_owned()
                } else {
                    value.to_owned()
                };
                cmd.env(key, final_value);
            }
        }
    }
}

fn make_android_cmd(cargo_args: Vec<String>, config: RsxConfig) -> Command {
    let ndk_root = resolve_ndk_root();
    let backend_value = backend_as_str(config.backend.unwrap_or_default());
    let mut cmd = Command::new("cargo");
    cmd.args(cargo_args)
        .env("RSX_RENDERER_BACKEND", backend_value);
    if let Some(ndk) = ndk_root {
        cmd.env("ANDROID_NDK_ROOT", ndk);
    }
    load_dotenv(&mut cmd);
    cmd
}

fn android_install_and_launch(args: &[String]) {
    let apk = apk_path(args);
    let status = Command::new("adb")
        .args(["install", "-r", apk.to_str().unwrap_or_default()])
        .status()
        .expect("[cargo-rsx] failed to invoke adb");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    let package_id = android_package_id(args);
    let component = format!("{package_id}/android.app.NativeActivity");
    let status = Command::new("adb")
        .args(["shell", "am", "start", "-n", &component])
        .status()
        .expect("[cargo-rsx] failed to invoke adb");
    std::process::exit(status.code().unwrap_or(1));
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
            "rs" | "rsx" | "toml"
        )
    })
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

fn hot_reload_rustflags() -> String {
    let existing = std::env::var("RUSTFLAGS").unwrap_or_default();
    // Adding --cfg=rsx_hot_reload changes the Cargo fingerprint, forcing a recompile so the proc macro re-runs with RSX_HOT_RELOAD_BUILD=1 and generates the hot reload code.
    let flag = "--cfg=rsx_hot_reload";
    if existing.is_empty() {
        flag.to_string()
    } else {
        format!("{existing} {flag}")
    }
}

fn preview_rustflags() -> String {
    // --cfg=rsx_preview is in the fingerprint so Cargo always recompiles when switching between dev and preview, ensuring the proc-macro-generated _rsx_hot_create_app includes or omits the preview branch correctly.
    format!("{} --cfg=rsx_preview", hot_reload_rustflags())
}

fn package_lib_path(workspace_root: &Path, package_name: &str, profile: &str) -> PathBuf {
    let lib_name = package_name.replace('-', "_");
    #[cfg(target_os = "macos")]
    let ext = "dylib";
    #[cfg(not(target_os = "macos"))]
    let ext = "so";
    workspace_root
        .join("target")
        .join(profile)
        .join(format!("lib{lib_name}.{ext}"))
}

fn package_bin_path(workspace_root: &Path, package_name: &str, profile: &str) -> PathBuf {
    workspace_root
        .join("target")
        .join(profile)
        .join(package_name)
}

#[cfg(unix)]
fn notify_hot_reload(socket_path: &str, lib_path: &str) {
    send_socket_message(socket_path, &format!("hot:{lib_path}"));
}

#[cfg(unix)]
fn notify_build_error(socket_path: &str, message: &str) {
    // Flatten multi-line error to a single line for the simple line-based protocol
    let single_line = message.replace('\n', " | ").replace('\r', "");
    send_socket_message(socket_path, &format!("err:{single_line}"));
}

#[cfg(unix)]
fn send_socket_message(socket_path: &str, message: &str) {
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    // Retry a few times — app may still be initializing the socket
    for attempt in 0..10 {
        match UnixStream::connect(socket_path) {
            Ok(mut stream) => {
                if let Err(e) = writeln!(stream, "{message}") {
                    eprintln!("[cargo-rsx] Failed to write to socket: {e}");
                }
                return;
            }
            Err(_) if attempt < 9 => {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(e) => {
                eprintln!("[cargo-rsx] Could not reach app socket: {e}");
            }
        }
    }
}

fn make_watcher(
    tx: mpsc::Sender<notify::Result<notify::Event>>,
    workspace_root: &Path,
) -> RecommendedWatcher {
    let mut watcher = RecommendedWatcher::new(tx, NotifyConfig::default())
        .expect("[cargo-rsx] failed to create file watcher");
    for src_dir in collect_src_dirs(workspace_root) {
        watcher
            .watch(&src_dir, RecursiveMode::Recursive)
            .unwrap_or_else(|e| {
                eprintln!(
                    "[cargo-rsx] warning: could not watch {}: {e}",
                    src_dir.display()
                )
            });
    }
    watcher
}

#[cfg(unix)]
fn watch_and_hot_reload(
    build_args: Vec<String>,
    bin_path: PathBuf,
    lib_path: PathBuf,
    socket_path: String,
    envs: Vec<(String, String)>,
    rustflags: String,
    workspace_root: PathBuf,
) -> ! {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let _watcher = make_watcher(tx, &workspace_root);

    eprintln!("[cargo-rsx] Starting with hot reload...");
    let mut child = Command::new(&bin_path)
        .env("RSX_HOT_LIB", lib_path.to_str().unwrap_or_default())
        .env("RSX_HOT_SOCKET", &socket_path)
        .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .spawn()
        .expect("[cargo-rsx] failed to spawn app binary");

    let debounce = Duration::from_millis(200);
    let mut last_event = Instant::now();
    let mut pending_rebuild = false;

    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                eprintln!("[cargo-rsx] App exited.");
                std::process::exit(0);
            }
            Ok(None) => {}
            Err(e) => eprintln!("[cargo-rsx] error: {e}"),
        }

        while let Ok(Ok(event)) = rx.try_recv() {
            if is_source_event(&event) {
                last_event = Instant::now();
                pending_rebuild = true;
            }
        }

        if pending_rebuild && last_event.elapsed() >= debounce {
            pending_rebuild = false;
            while rx.try_recv().is_ok() {}
            eprintln!("[cargo-rsx] Change detected, rebuilding...");
            let output = Command::new("cargo")
                .args(&build_args)
                .env("RSX_HOT_RELOAD_BUILD", "1")
                .env("RUSTFLAGS", &rustflags)
                .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
                .output()
                .expect("[cargo-rsx] failed to invoke cargo");
            if output.status.success() {
                notify_hot_reload(&socket_path, lib_path.to_str().unwrap_or_default());
                eprintln!("[cargo-rsx] Hot reloaded.");
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("{stderr}");
                eprintln!("[cargo-rsx] Build failed, waiting for changes...");
                notify_build_error(&socket_path, &stderr);
            }
        }

        if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(50)) {
            if is_source_event(&event) {
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
        eprintln!("[cargo-rsx] Starting...");
        let mut child = Command::new("cargo")
            .args(&cargo_args)
            .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .spawn()
            .expect("[cargo-rsx] failed to spawn cargo");

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
                    eprintln!("[cargo-rsx] Process exited ({code}). Watching for changes...");
                    loop {
                        match rx.recv() {
                            Ok(Ok(event)) if is_source_event(&event) => {
                                while rx.try_recv().is_ok() {}
                                eprintln!("[cargo-rsx] Change detected, restarting...");
                                break 'watch;
                            }
                            _ => {}
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => eprintln!("[cargo-rsx] error: {e}"),
            }

            while let Ok(Ok(event)) = rx.try_recv() {
                if is_source_event(&event) {
                    last_event = Instant::now();
                    pending_restart = true;
                }
            }

            if pending_restart && last_event.elapsed() >= debounce {
                while rx.try_recv().is_ok() {}
                eprintln!("[cargo-rsx] Change detected, restarting...");
                child.kill().ok();
                child.wait().ok();
                break 'watch;
            }

            if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(50)) {
                if is_source_event(&event) {
                    last_event = Instant::now();
                    pending_restart = true;
                }
            }
        }
    }
}

pub fn run(args: Vec<String>) {
    let cli = Cli::parse_from(std::iter::once("cargo-rsx".to_string()).chain(args));
    match cli.command.unwrap_or_else(default_dev_command) {
        RsxCommand::Dev(args) => run_dev_cmd(args),
        RsxCommand::Preview(args) => run_preview_cmd(args),
        RsxCommand::Build(args) => run_build_cmd(args),
        RsxCommand::Test(args) => run_test_cmd(args),
        RsxCommand::New { name } => {
            eprintln!("[cargo-rsx] `cargo rsx new {name}` is not yet implemented.");
            std::process::exit(1);
        }
        RsxCommand::Doctor => run_doctor_cmd(),
    }
}

// No subcommand behaves like `cargo rsx dev` with default flags.
fn default_dev_command() -> RsxCommand {
    RsxCommand::Dev(DevArgs {
        hot: HotArgs {
            common: CommonArgs {
                package: None,
                features: None,
                target: Target::Desktop,
                backend: None,
                cargo_args: vec![],
            },
            release: false,
            no_hot_reload: false,
        },
        devtools: None,
    })
}

struct Doctor {
    warnings: usize,
    errors: usize,
}

impl Doctor {
    fn new() -> Self {
        Self {
            warnings: 0,
            errors: 0,
        }
    }

    fn section(&self, title: &str) {
        println!("\n{title}");
    }

    fn info(&self, label: &str, detail: &str) {
        println!("  \u{2022} {label}: {detail}");
    }

    fn ok(&self, label: &str, detail: &str) {
        println!("  \u{2713} {label}: {detail}");
    }

    fn warn(&mut self, label: &str, detail: &str) {
        self.warnings += 1;
        println!("  \u{26a0} {label}: {detail}");
    }

    fn fail(&mut self, label: &str, detail: &str) {
        self.errors += 1;
        println!("  \u{2717} {label}: {detail}");
    }
}

fn command_first_line(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stream = if output.stdout.is_empty() {
        output.stderr
    } else {
        output.stdout
    };
    let text = String::from_utf8_lossy(&stream);
    Some(text.lines().next().unwrap_or("").trim().to_string())
}

fn android_sdk_root() -> Option<PathBuf> {
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(value) = std::env::var(var)
            && !value.is_empty()
        {
            return Some(PathBuf::from(value));
        }
    }
    None
}

fn installed_android_platforms(sdk_root: &Path) -> Vec<u32> {
    let mut versions: Vec<u32> = std::fs::read_dir(sdk_root.join("platforms"))
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name();
            name.to_str()?.strip_prefix("android-")?.parse::<u32>().ok()
        })
        .collect();
    versions.sort_unstable();
    versions
}

fn rustup_has_target(target: &str) -> Option<bool> {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(text.lines().any(|line| line.trim() == target))
}

fn run_doctor_cmd() -> ! {
    let mut doc = Doctor::new();
    println!("cargo rsx doctor");

    doc.section("Toolchain");
    match command_first_line("cargo", &["--version"]) {
        Some(v) => doc.ok("cargo", &v),
        None => doc.fail("cargo", "not found on PATH"),
    }
    match command_first_line("rustc", &["--version"]) {
        Some(v) => doc.ok("rustc", &v),
        None => doc.fail("rustc", "not found on PATH"),
    }

    doc.section("Project");
    let cwd = std::env::current_dir().unwrap_or_default();
    match rsx_workspace::find_workspace_root(&cwd) {
        Some(root) => doc.info("workspace root", &root.display().to_string()),
        None => doc.info("workspace root", "none (standalone package)"),
    }
    let config = load_config(&[]);
    let package_dir = find_package_dir(&[]);
    let rsx_toml = package_dir.join("rsx.toml");
    if rsx_toml.exists() {
        doc.ok("rsx.toml", &rsx_toml.display().to_string());
    } else {
        doc.info("rsx.toml", "not found (using defaults)");
    }
    let has_manifest_rsx = std::fs::read_to_string(package_dir.join("Cargo.toml"))
        .ok()
        .and_then(|c| toml::from_str::<CargoManifest>(&c).ok())
        .and_then(|m| m.package)
        .and_then(|p| p.metadata)
        .and_then(|m| m.rsx)
        .is_some();
    doc.info(
        "[package.metadata.rsx]",
        if has_manifest_rsx {
            "present"
        } else {
            "not set"
        },
    );
    doc.info(
        "config precedence",
        "CLI flags > rsx.toml > [package.metadata.rsx] > defaults",
    );

    doc.section("Desktop");
    let backend = config
        .backend
        .map(backend_as_str)
        .unwrap_or("auto (default)");
    doc.info("configured backend", backend);
    doc.info("software renderer", "always available");
    doc.info(
        "hardware renderer",
        "needs a working GPU/driver, verified at runtime",
    );

    doc.section("Android (only needed for --target android)");
    match android_sdk_root() {
        Some(sdk) if sdk.exists() => {
            doc.ok("Android SDK", &sdk.display().to_string());
            let platforms = installed_android_platforms(&sdk);
            if platforms.is_empty() {
                doc.warn(
                    "installed SDK platforms",
                    "none found — `sdkmanager \"platforms;android-36\"`",
                );
            } else {
                let list = platforms
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                doc.info("installed SDK platforms", &list);
            }
        }
        Some(sdk) => doc.warn(
            "Android SDK",
            &format!("{} (path does not exist)", sdk.display()),
        ),
        None => doc.warn("Android SDK", "not set (ANDROID_HOME / ANDROID_SDK_ROOT)"),
    }
    match resolve_ndk_root() {
        Some(ndk) => doc.ok("Android NDK", &ndk),
        None => doc.warn(
            "Android NDK",
            "not found (ANDROID_NDK_ROOT or $ANDROID_HOME/ndk/*)",
        ),
    }
    match command_first_line("cargo", &["apk", "--version"]) {
        Some(v) => doc.ok("cargo-apk", &v),
        None => doc.warn("cargo-apk", "not found (`cargo install cargo-apk`)"),
    }
    match rustup_has_target("aarch64-linux-android") {
        Some(true) => doc.ok("rust target aarch64-linux-android", "installed"),
        Some(false) => doc.warn(
            "rust target aarch64-linux-android",
            "missing (`rustup target add aarch64-linux-android`)",
        ),
        None => doc.warn(
            "rust target aarch64-linux-android",
            "rustup not found, cannot verify",
        ),
    }
    match command_first_line("adb", &["--version"]) {
        Some(v) => doc.ok("adb", &v),
        None => doc.warn("adb", "not found (Android platform-tools)"),
    }

    println!();
    if doc.errors > 0 {
        println!(
            "\u{2717} {} error(s), {} warning(s)",
            doc.errors, doc.warnings
        );
        std::process::exit(1);
    } else if doc.warnings > 0 {
        println!("\u{26a0} {} warning(s)", doc.warnings);
        std::process::exit(0);
    }
    println!("\u{2713} everything looks good");
    std::process::exit(0);
}

fn run_dev_cmd(args: DevArgs) {
    let DevArgs { hot, devtools } = args;
    let HotArgs {
        common,
        release,
        no_hot_reload,
    } = hot;
    let CommonArgs {
        package,
        features,
        target,
        backend,
        cargo_args: extra,
    } = common;
    let mut cargo_args = build_cargo_args(&package, release, &features);
    cargo_args.extend(extra);
    if matches!(target, Target::Android) {
        cargo_args.push("--android".to_string());
    }
    let mut config = load_config(&cargo_args);
    if let Some(backend) = backend {
        config.backend = Some(backend_from_arg(backend));
    }
    // CLI `--devtools off` overrides any config-file setting.
    if let Some(devtools) = devtools {
        config.dev.get_or_insert_with(Default::default).devtools =
            Some(matches!(devtools, DevtoolsArg::On));
    }
    run_hot_loop(
        HotMode::Dev,
        HotLoopOpts {
            args: cargo_args,
            config,
            no_hot_reload,
        },
    );
}

fn run_preview_cmd(args: PreviewArgs) {
    let PreviewArgs {
        hot,
        component,
        list,
    } = args;
    if list {
        eprintln!("[cargo-rsx] --list is not yet implemented (requires project scan).");
        std::process::exit(1);
    }
    // The preview host process inherits our env; it filters PreviewEntries by this when set.
    if let Some(component) = &component {
        // SAFETY: single-threaded at this point (set before any threads/spawns are created).
        unsafe { std::env::set_var("RSX_PREVIEW_COMPONENT", component) };
    }
    let HotArgs {
        common,
        release,
        no_hot_reload,
    } = hot;
    let CommonArgs {
        package,
        features,
        target,
        backend,
        cargo_args: extra,
    } = common;
    let mut cargo_args = build_cargo_args(&package, release, &features);
    cargo_args.extend(extra);
    if matches!(target, Target::Android) {
        cargo_args.push("--android".to_string());
    }
    let mut config = load_config(&cargo_args);
    if let Some(backend) = backend {
        config.backend = Some(backend_from_arg(backend));
    }
    run_hot_loop(
        HotMode::Preview,
        HotLoopOpts {
            args: cargo_args,
            config,
            no_hot_reload,
        },
    );
}

fn run_test_cmd(args: TestArgs) -> ! {
    let TestArgs { common, release } = args;
    let CommonArgs {
        package,
        features,
        target,
        backend,
        cargo_args: extra,
    } = common;
    if matches!(target, Target::Android) {
        eprintln!(
            "[cargo-rsx] `cargo rsx test` renders on the host; --target android is not supported."
        );
        std::process::exit(2);
    }
    // Run the app binary in test mode: RSX_TEST makes the generated entrypoint render every preview headlessly and exit non-zero on any failure, instead of opening a window.
    let mut cargo_args = vec!["run".to_string()];
    cargo_args.extend(build_cargo_args(&package, release, &features));
    cargo_args.extend(extra);
    // No RSX_RENDERER_BACKEND: the test host never instantiates a renderer, and that value is read via option_env!, so setting it would change the build fingerprint and force a needless recompile.
    let _ = backend;
    eprintln!("[cargo-rsx] Running component render tests...");
    let status = Command::new("cargo")
        .args(&cargo_args)
        .env("RSX_TEST", "1")
        .status()
        .expect("[cargo-rsx] failed to invoke cargo");
    std::process::exit(status.code().unwrap_or(1));
}

fn build_format_name(format: &BuildFormat) -> &'static str {
    match format {
        BuildFormat::Appimage => "appimage",
        BuildFormat::Deb => "deb",
        BuildFormat::Dmg => "dmg",
        BuildFormat::Apk => "apk",
        BuildFormat::Dir => "dir",
    }
}

fn run_build_cmd(args: BuildArgs) -> ! {
    let BuildArgs { common, format } = args;
    let CommonArgs {
        package,
        features,
        target,
        backend,
        cargo_args: extra,
    } = common;
    let mut android = matches!(target, Target::Android);

    // Today only `dir` (desktop) and `apk` (android) are wired; the native installer formats stay explicit stubs. `--format apk` implies Android; `--format dir` is desktop-only.
    match &format {
        Some(fmt @ (BuildFormat::Deb | BuildFormat::Dmg | BuildFormat::Appimage)) => {
            eprintln!(
                "[cargo-rsx] Packaging format `{}` is not yet implemented. Supported today: `dir` (desktop) and `apk` (android).",
                build_format_name(fmt)
            );
            std::process::exit(1);
        }
        Some(BuildFormat::Apk) => android = true,
        Some(BuildFormat::Dir) if android => {
            eprintln!(
                "[cargo-rsx] `--format dir` is desktop-only; use `--target android` (or `--format apk`) for Android."
            );
            std::process::exit(2);
        }
        Some(BuildFormat::Dir) | None => {}
    }

    // Build always implies --release.
    let mut cargo_args = build_cargo_args(&package, true, &features);
    cargo_args.extend(extra);
    if android {
        cargo_args.push("--android".to_string());
    }
    let mut config = load_config(&cargo_args);
    if let Some(backend) = backend {
        config.backend = Some(backend_from_arg(backend));
    }

    if android {
        build_android_package(cargo_args, config)
    } else {
        build_desktop_dir(cargo_args, config)
    }
}

fn build_desktop_dir(cargo_args: Vec<String>, config: RsxConfig) -> ! {
    let (_android, rest) = split_android_flag(cargo_args);
    let backend_value = backend_as_str(config.backend.unwrap_or_default());

    let mut build_args = vec!["build".to_string()];
    build_args.extend(rest.clone());
    if !build_args.contains(&"--release".to_string()) {
        build_args.push("--release".to_string());
    }
    eprintln!("[cargo-rsx] Building release binary...");
    let status = Command::new("cargo")
        .args(&build_args)
        .env("RSX_RENDERER_BACKEND", backend_value)
        .status()
        .expect("[cargo-rsx] failed to invoke cargo");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    let package_dir = find_package_dir(&rest);
    let workspace_root =
        rsx_workspace::find_workspace_root(&package_dir).unwrap_or(package_dir.clone());
    let package_name = read_package_manifest(&rest)
        .map(|p| p.name)
        .unwrap_or_else(|| "app".to_string());
    let bin_path = package_bin_path(&workspace_root, &package_name, "release");
    if !bin_path.exists() {
        eprintln!(
            "[cargo-rsx] Build succeeded but no binary was found at {}. Does this package produce a `[[bin]]`?",
            bin_path.display()
        );
        std::process::exit(1);
    }

    // Distribution bundle lives under target/ (a build artifact, next to the binary it packages), so it inherits target's gitignore and is not confused with .rsx/ (which is generated source).
    let dist_dir = workspace_root
        .join("target")
        .join("rsx-dist")
        .join(&package_name);
    if let Err(e) = std::fs::create_dir_all(&dist_dir) {
        eprintln!("[cargo-rsx] Failed to create {}: {e}", dist_dir.display());
        std::process::exit(1);
    }
    if let Err(e) = std::fs::copy(&bin_path, dist_dir.join(&package_name)) {
        eprintln!("[cargo-rsx] Failed to copy binary into dist: {e}");
        std::process::exit(1);
    }
    // Assets are embedded in the binary today (rsx has no exe-relative asset lookup), so the bundle is just the self-contained executable. See the "disk assets" TODO for the disk-asset story that would add an assets/ copy here.

    eprintln!(
        "[cargo-rsx] Packaged desktop build at {}",
        dist_dir.display()
    );
    std::process::exit(0);
}

fn build_android_package(cargo_args: Vec<String>, config: RsxConfig) -> ! {
    let (_android, rest) = split_android_flag(cargo_args);
    let mut build_args = vec!["apk".to_string(), "build".to_string(), "--lib".to_string()];
    build_args.extend(rest.clone());
    if !build_args.contains(&"--release".to_string()) {
        build_args.push("--release".to_string());
    }
    let status = make_android_cmd(build_args, config)
        .status()
        .expect("[cargo-rsx] failed to invoke cargo");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    let apk = apk_path(&rest);
    let package_dir = find_package_dir(&rest);
    let workspace_root = rsx_workspace::find_workspace_root(&package_dir).unwrap_or(package_dir);
    let dist_dir = workspace_root.join("target").join("rsx-dist");
    let _ = std::fs::create_dir_all(&dist_dir);
    if !apk.exists() {
        eprintln!(
            "[cargo-rsx] APK build finished but {} was not found.",
            apk.display()
        );
        std::process::exit(1);
    }
    let dest = dist_dir.join(apk.file_name().unwrap_or_default());
    if let Err(e) = std::fs::copy(&apk, &dest) {
        eprintln!("[cargo-rsx] Built APK but failed to copy into dist: {e}");
        std::process::exit(1);
    }
    eprintln!("[cargo-rsx] Packaged APK at {}", dest.display());
    std::process::exit(0);
}

fn build_cargo_args(
    package: &Option<String>,
    release: bool,
    features: &Option<String>,
) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(pkg) = package {
        args.push("-p".to_string());
        args.push(pkg.clone());
    }
    if release {
        args.push("--release".to_string());
    }
    if let Some(features) = features {
        args.push("--features".to_string());
        args.push(features.clone());
    }
    args
}

fn backend_from_arg(backend: BackendArg) -> RendererBackend {
    match backend {
        BackendArg::Auto => RendererBackend::Auto,
        BackendArg::Hardware => RendererBackend::Hardware,
        BackendArg::Software => RendererBackend::Software,
    }
}

enum HotMode {
    Dev,
    Preview,
}

impl HotMode {
    fn is_preview(&self) -> bool {
        matches!(self, HotMode::Preview)
    }

    fn features(&self) -> &'static [&'static str] {
        match self {
            HotMode::Dev => &["rsx/dev"],
            HotMode::Preview => &["rsx/preview", "rsx/dev"],
        }
    }

    fn rustflags(&self) -> String {
        match self {
            HotMode::Dev => hot_reload_rustflags(),
            HotMode::Preview => preview_rustflags(),
        }
    }
}

struct HotLoopOpts {
    args: Vec<String>,
    config: RsxConfig,
    no_hot_reload: bool,
}

fn run_hot_loop(mode: HotMode, opts: HotLoopOpts) -> ! {
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
            .expect("[cargo-rsx] failed to invoke cargo");
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }

        android_install_and_launch(&rest);
        std::process::exit(0);
    }

    let mut launch_envs = vec![(
        "RSX_RENDERER_BACKEND".to_string(),
        backend_value.to_string(),
    )];
    if is_preview {
        launch_envs.push(("RSX_PREVIEW".to_string(), "1".to_string()));
    }
    let devtools_disabled = config.dev.as_ref().and_then(|d| d.devtools) == Some(false);
    if devtools_disabled {
        launch_envs.push(("RSX_DEVTOOLS".to_string(), "0".to_string()));
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

    let package_dir = find_package_dir(&rest);
    let workspace_root =
        rsx_workspace::find_workspace_root(&package_dir).unwrap_or(package_dir.clone());
    let profile = if rest.contains(&"--release".to_string()) {
        "release"
    } else {
        "debug"
    };

    #[cfg(unix)]
    if !no_hot_reload {
        let rustflags = mode.rustflags();
        let package_name = read_package_manifest(&rest)
            .map(|p| p.name)
            .unwrap_or_else(|| "app".to_string());
        let lib_path = package_lib_path(&workspace_root, &package_name, profile);
        let bin_path = package_bin_path(&workspace_root, &package_name, profile);

        // Initial build (produces both binary and dylib).
        let mut build_args = vec!["build".to_string()];
        build_args.extend(rest.clone());
        for feature in features {
            inject_feature(&mut build_args, feature);
        }
        eprintln!("[cargo-rsx] Building...");
        let mut build_cmd = Command::new("cargo");
        build_cmd
            .args(&build_args)
            .env("RSX_HOT_RELOAD_BUILD", "1")
            .env("RUSTFLAGS", &rustflags);
        if is_preview {
            build_cmd.env("RSX_PREVIEW_BUILD", "1");
        }
        let status = build_cmd
            .status()
            .expect("[cargo-rsx] failed to invoke cargo");
        if !status.success() {
            eprintln!("[cargo-rsx] Initial build failed. Watching for changes...");
        }

        if bin_path.exists() && lib_path.exists() {
            let lib_build_args = make_lib_build_args(&rest, features);

            let mut build_envs = launch_envs.clone();
            if is_preview {
                build_envs.push(("RSX_PREVIEW_BUILD".to_string(), "1".to_string()));
            }

            let socket_path = format!("/tmp/rsx-hot-{}.sock", std::process::id());
            watch_and_hot_reload(
                lib_build_args,
                bin_path,
                lib_path,
                socket_path,
                build_envs,
                rustflags,
                workspace_root,
            );
        }
        // Fallback if binary or lib not found: use process-restart watch.
    }

    watch_and_run(cargo_args, launch_envs, workspace_root);
}

fn backend_as_str(backend: RendererBackend) -> &'static str {
    match backend {
        RendererBackend::Auto => "auto",
        RendererBackend::Hardware => "hardware",
        RendererBackend::Software => "software",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsx_toml_overrides_manifest_field_by_field() {
        let manifest = RsxConfig {
            backend: Some(RendererBackend::Software),
            dev: Some(DevConfig {
                window: Some(WindowConfig {
                    width: Some(800),
                    height: Some(600),
                    fullscreen: Some("disabled".to_string()),
                    ..Default::default()
                }),
                devtools: Some(true),
            }),
        };
        let file = RsxConfig {
            backend: Some(RendererBackend::Hardware),
            dev: Some(DevConfig {
                // rsx.toml sets width + position but omits height/fullscreen/devtools.
                window: Some(WindowConfig {
                    width: Some(1024),
                    position: Some("10,20".to_string()),
                    ..Default::default()
                }),
                devtools: None,
            }),
        };

        let merged = merge_config(manifest, file);
        assert!(matches!(merged.backend, Some(RendererBackend::Hardware)));
        let dev = merged.dev.unwrap();
        assert_eq!(dev.devtools, Some(true)); // omitted in rsx.toml → manifest value survives
        let window = dev.window.unwrap();
        assert_eq!(window.width, Some(1024)); // rsx.toml wins
        assert_eq!(window.height, Some(600)); // falls back to manifest
        assert_eq!(window.position.as_deref(), Some("10,20")); // only in rsx.toml
        assert_eq!(window.fullscreen.as_deref(), Some("disabled")); // only in manifest
    }

    #[test]
    fn manifest_used_when_rsx_toml_absent() {
        let manifest = RsxConfig {
            backend: Some(RendererBackend::Software),
            dev: None,
        };
        let merged = merge_config(manifest, RsxConfig::default());
        assert!(matches!(merged.backend, Some(RendererBackend::Software)));
        assert!(merged.dev.is_none());
    }
}
