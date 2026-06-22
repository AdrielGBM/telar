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
    /// Run tests (not yet implemented)
    Test,
    /// Create a new RSX project (not yet implemented)
    New {
        /// Project name
        name: String,
    },
    /// Check the development environment (not yet implemented)
    Doctor,
}

#[derive(clap::Args)]
struct DevArgs {
    /// Package to run
    #[arg(short = 'p', long)]
    package: Option<String>,
    /// Build in release mode
    #[arg(long)]
    release: bool,
    /// Additional Cargo features
    #[arg(short = 'F', long, value_name = "FEATURES")]
    features: Option<String>,
    /// Target platform
    #[arg(long, value_enum, default_value = "desktop")]
    target: Target,
    /// Renderer backend
    #[arg(long, value_enum)]
    backend: Option<BackendArg>,
    /// Disable hot reload, restart process on changes instead
    #[arg(long)]
    no_hot_reload: bool,
    /// Devtools overlay
    #[arg(long, value_enum)]
    devtools: Option<DevtoolsArg>,
    /// Extra args passed directly to cargo (after --)
    #[arg(last = true)]
    cargo_args: Vec<String>,
}

#[derive(clap::Args)]
struct PreviewArgs {
    /// Package to run
    #[arg(short = 'p', long)]
    package: Option<String>,
    /// Build in release mode
    #[arg(long)]
    release: bool,
    /// Additional Cargo features
    #[arg(short = 'F', long, value_name = "FEATURES")]
    features: Option<String>,
    /// Target platform
    #[arg(long, value_enum, default_value = "desktop")]
    target: Target,
    /// Renderer backend
    #[arg(long, value_enum)]
    backend: Option<BackendArg>,
    /// Disable hot reload
    #[arg(long)]
    no_hot_reload: bool,
    /// Preview a specific component by name
    #[arg(long)]
    component: Option<String>,
    /// List all available previews and exit
    #[arg(long)]
    list: bool,
    /// Extra args passed directly to cargo (after --)
    #[arg(last = true)]
    cargo_args: Vec<String>,
}

#[derive(clap::Args)]
struct BuildArgs {
    /// Package to build
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
    /// Output package format
    #[arg(long, value_name = "FORMAT")]
    format: Option<BuildFormat>,
    /// Output directory
    #[arg(long, value_name = "DIR")]
    out: Option<String>,
    /// Extra args passed directly to cargo (after --)
    #[arg(last = true)]
    cargo_args: Vec<String>,
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
}

#[derive(Deserialize, Default, Clone)]
struct DevConfig {
    #[serde(default)]
    pub window: Option<WindowConfig>,
    #[serde(default)]
    pub devtools: Option<bool>,
}

#[derive(Deserialize, Default, Clone)]
struct BuildConfig {
    pub format: Option<String>,
    pub out: Option<String>,
}

#[derive(Deserialize, Default)]
struct RsxConfig {
    #[serde(default)]
    pub backend: Option<RendererBackend>,
    #[serde(default)]
    pub devtools: Option<bool>,
    #[serde(default)]
    pub window: Option<WindowConfig>,
    #[serde(default)]
    pub dev: Option<DevConfig>,
    #[serde(default)]
    pub build: Option<BuildConfig>,
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
}
#[derive(Deserialize, Default)]
struct AndroidMetadata {
    package: Option<String>,
}

fn find_workspace_root(dir: &Path) -> Option<PathBuf> {
    let mut current = dir.to_path_buf();
    loop {
        let manifest_path = current.join("Cargo.toml");
        if let Ok(content) = std::fs::read_to_string(&manifest_path)
            && let Ok(manifest) = toml::from_str::<CargoManifest>(&content)
            && manifest.workspace.is_some()
        {
            return Some(current);
        }
        match current.parent() {
            Some(p) => current = p.to_path_buf(),
            None => return None,
        }
    }
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
        if let Some(root) = find_workspace_root(&cwd)
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
    let workspace_root = find_workspace_root(&crate_dir).unwrap_or(crate_dir.clone());
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

fn load_config(args: &[String]) -> RsxConfig {
    let path = find_package_dir(args).join("rsx.toml");
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

fn apply_dev_window_env(envs: &mut Vec<(String, String)>, w: &WindowConfig) {
    if let Some(title) = &w.title {
        envs.push(("RSX_DEV_WINDOW_TITLE".to_string(), title.clone()));
    }
    if let Some(width) = w.width {
        envs.push(("RSX_DEV_WINDOW_WIDTH".to_string(), width.to_string()));
    }
    if let Some(height) = w.height {
        envs.push(("RSX_DEV_WINDOW_HEIGHT".to_string(), height.to_string()));
    }
    if let Some(dec) = w.decorations {
        envs.push((
            "RSX_DEV_WINDOW_DECORATIONS".to_string(),
            if dec { "1" } else { "0" }.to_string(),
        ));
    }
    if let Some(res) = w.resizable {
        envs.push((
            "RSX_DEV_WINDOW_RESIZABLE".to_string(),
            if res { "1" } else { "0" }.to_string(),
        ));
    }
    if let Some(tr) = w.transparent {
        envs.push((
            "RSX_DEV_WINDOW_TRANSPARENT".to_string(),
            if tr { "1" } else { "0" }.to_string(),
        ));
    }
}

fn load_dotenv(cmd: &mut Command) {
    let cwd = std::env::current_dir().unwrap_or_default();
    let root = find_workspace_root(&cwd).unwrap_or(cwd);
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
            // Only set if not already set in the calling environment.
            if std::env::var(key).is_err() {
                // Resolve relative paths against the workspace root so values like
                // "android-release.keystore" work regardless of the cwd at signing time.
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
    let backend_value = backend_str(config.backend.unwrap_or_default());
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
    // Adding --cfg=rsx_hot_reload changes the Cargo fingerprint, forcing a recompile so
    // the proc macro re-runs with RSX_HOT_RELOAD_BUILD=1 and generates the hot reload code.
    let flag = "--cfg=rsx_hot_reload";
    if existing.is_empty() {
        flag.to_string()
    } else {
        format!("{existing} {flag}")
    }
}

fn preview_rustflags() -> String {
    // --cfg=rsx_preview is in the fingerprint so Cargo always recompiles the crate when
    // switching between dev and preview, ensuring the proc-macro-generated _rsx_hot_create_app
    // includes or omits the preview branch correctly.
    format!("{} --cfg=rsx_preview", hot_reload_rustflags())
}

fn package_lib_path(workspace_root: &Path, pkg_name: &str, profile: &str) -> PathBuf {
    let lib_name = pkg_name.replace('-', "_");
    #[cfg(target_os = "macos")]
    let ext = "dylib";
    #[cfg(not(target_os = "macos"))]
    let ext = "so";
    workspace_root
        .join("target")
        .join(profile)
        .join(format!("lib{lib_name}.{ext}"))
}

fn package_bin_path(workspace_root: &Path, pkg_name: &str, profile: &str) -> PathBuf {
    workspace_root.join("target").join(profile).join(pkg_name)
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
    let mut watcher = RecommendedWatcher::new(tx, NotifyConfig::default())
        .expect("[cargo-rsx] failed to create file watcher");
    for src_dir in collect_src_dirs(&workspace_root) {
        watcher
            .watch(&src_dir, RecursiveMode::Recursive)
            .unwrap_or_else(|e| {
                eprintln!(
                    "[cargo-rsx] warning: could not watch {}: {e}",
                    src_dir.display()
                )
            });
    }

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

        // Block until next event or timeout (replaces the sleep)
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
    let mut watcher = RecommendedWatcher::new(tx, NotifyConfig::default())
        .expect("[cargo-rsx] failed to create file watcher");

    for src_dir in collect_src_dirs(&workspace_root) {
        watcher
            .watch(&src_dir, RecursiveMode::Recursive)
            .unwrap_or_else(|e| {
                eprintln!(
                    "[cargo-rsx] warning: could not watch {}: {e}",
                    src_dir.display()
                )
            });
    }

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
                    // Block until a source file changes, then restart
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

            // Block until next event or timeout (replaces the sleep)
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
    match cli.command.unwrap_or(RsxCommand::Dev(DevArgs {
        package: None,
        release: false,
        features: None,
        target: Target::Desktop,
        backend: None,
        no_hot_reload: false,
        devtools: None,
        cargo_args: vec![],
    })) {
        RsxCommand::Dev(args) => run_dev_cmd(args),
        RsxCommand::Preview(args) => run_preview_cmd(args),
        RsxCommand::Build(args) => run_build_cmd(args),
        RsxCommand::Test => {
            eprintln!("[cargo-rsx] `cargo rsx test` is not yet implemented.");
            std::process::exit(1);
        }
        RsxCommand::New { name } => {
            eprintln!("[cargo-rsx] `cargo rsx new {name}` is not yet implemented.");
            std::process::exit(1);
        }
        RsxCommand::Doctor => {
            eprintln!("[cargo-rsx] `cargo rsx doctor` is not yet implemented.");
            std::process::exit(1);
        }
    }
}

fn run_dev_cmd(args: DevArgs) {
    let mut cargo_args = build_cargo_args(&args.package, args.release, &args.features);
    cargo_args.extend(args.cargo_args);
    if matches!(args.target, Target::Android) {
        cargo_args.push("--android".to_string());
    }
    let mut config = load_config(&cargo_args);
    if let Some(backend) = args.backend {
        config.backend = Some(backend_from_arg(backend));
    }
    // CLI `--devtools off` overrides any rsx.toml setting.
    if let Some(devtools) = args.devtools {
        config.devtools = Some(matches!(devtools, DevtoolsArg::On));
    }
    run_hot_loop(
        HotMode::Dev,
        HotLoopOpts {
            args: cargo_args,
            config,
            no_hot_reload: args.no_hot_reload,
        },
    );
}

fn run_preview_cmd(args: PreviewArgs) {
    if args.list {
        eprintln!("[cargo-rsx] --list is not yet implemented (requires project scan).");
        std::process::exit(1);
    }
    let mut cargo_args = build_cargo_args(&args.package, args.release, &args.features);
    cargo_args.extend(args.cargo_args);
    if matches!(args.target, Target::Android) {
        cargo_args.push("--android".to_string());
    }
    let mut config = load_config(&cargo_args);
    if let Some(backend) = args.backend {
        config.backend = Some(backend_from_arg(backend));
    }
    run_hot_loop(
        HotMode::Preview,
        HotLoopOpts {
            args: cargo_args,
            config,
            no_hot_reload: args.no_hot_reload,
        },
    );
}

fn run_build_cmd(args: BuildArgs) {
    if let Some(ref fmt) = args.format {
        let fmt_name = match fmt {
            BuildFormat::Appimage => "appimage",
            BuildFormat::Deb => "deb",
            BuildFormat::Dmg => "dmg",
            BuildFormat::Apk => "apk",
            BuildFormat::Dir => "dir",
        };
        eprintln!("[cargo-rsx] Packaging format `{fmt_name}` is not yet implemented.");
        std::process::exit(1);
    }
    eprintln!(
        "[cargo-rsx] Building release binary. Use --format <FORMAT> to package for distribution."
    );
    // Build always implies --release.
    let mut cargo_args = build_cargo_args(&args.package, true, &args.features);
    cargo_args.extend(args.cargo_args);
    let android = matches!(args.target, Target::Android);
    let mut config = load_config(&cargo_args);
    if let Some(backend) = args.backend {
        config.backend = Some(backend_from_arg(backend));
    }
    run_bundle_inner(android, cargo_args, config);
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
    if let Some(feats) = features {
        args.push("--features".to_string());
        args.push(feats.clone());
    }
    args
}

fn backend_from_arg(b: BackendArg) -> RendererBackend {
    match b {
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
    let backend_value = backend_str(config.backend.unwrap_or_default());
    let is_preview = mode.is_preview();

    if android {
        // `cargo apk run --lib` crashes on UID parsing when launching;
        // work around by doing build → adb install → adb shell am start manually.
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

    // Envs passed when launching the app binary (and to cargo run in fallback mode).
    let mut launch_envs = vec![(
        "RSX_RENDERER_BACKEND".to_string(),
        backend_value.to_string(),
    )];
    if is_preview {
        launch_envs.push(("RSX_PREVIEW".to_string(), "1".to_string()));
    }
    // Disable the devtools overlay when turned off via CLI flag or rsx.toml.
    let devtools_disabled = config.devtools == Some(false)
        || config.dev.as_ref().and_then(|d| d.devtools) == Some(false);
    if devtools_disabled {
        launch_envs.push(("RSX_DEVTOOLS".to_string(), "0".to_string()));
    }
    if let Some(dev) = &config.dev
        && let Some(w) = &dev.window
    {
        apply_dev_window_env(&mut launch_envs, w);
    }

    let mut cargo_args = vec!["run".to_string()];
    cargo_args.extend(rest.clone());
    for feature in features {
        inject_feature(&mut cargo_args, feature);
    }

    let pkg_dir = find_package_dir(&rest);
    let workspace_root = find_workspace_root(&pkg_dir).unwrap_or(pkg_dir.clone());
    let profile = if rest.contains(&"--release".to_string()) {
        "release"
    } else {
        "debug"
    };

    #[cfg(unix)]
    if !no_hot_reload {
        let rustflags = mode.rustflags();
        let pkg_name = read_package_manifest(&rest)
            .map(|p| p.name)
            .unwrap_or_else(|| "app".to_string());
        let lib_path = package_lib_path(&workspace_root, &pkg_name, profile);
        let bin_path = package_bin_path(&workspace_root, &pkg_name, profile);

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
            // Hot reload mode: only rebuild lib on changes.
            let lib_build_args = make_lib_build_args(&rest, features);

            // Build envs include launch envs plus preview build marker.
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

fn run_bundle_inner(android: bool, mut rest: Vec<String>, config: RsxConfig) -> ! {
    if !rest.contains(&"--release".to_string()) {
        rest.push("--release".to_string());
    }

    if android {
        let mut build_args = vec!["apk".to_string(), "build".to_string(), "--lib".to_string()];
        build_args.extend(rest);

        let status = make_android_cmd(build_args, config)
            .status()
            .expect("[cargo-rsx] failed to invoke cargo");
        std::process::exit(status.code().unwrap_or(1));
    } else {
        let backend_value = backend_str(config.backend.unwrap_or_default());

        let mut cargo_args = vec!["build".to_string()];
        cargo_args.extend(rest);

        let status = Command::new("cargo")
            .args(&cargo_args)
            .env("RSX_RENDERER_BACKEND", backend_value)
            .status()
            .expect("[cargo-rsx] failed to invoke cargo");

        std::process::exit(status.code().unwrap_or(1));
    }
}

fn backend_str(backend: RendererBackend) -> &'static str {
    match backend {
        RendererBackend::Auto => "auto",
        RendererBackend::Hardware => "hardware",
        RendererBackend::Software => "software",
    }
}
