use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rsx::RendererBackend;
use serde::Deserialize;

#[derive(Deserialize, Default)]
struct RsxConfig {
    #[serde(default)]
    pub backend: Option<RendererBackend>,
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
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!(
                "[cargo-rsx] Warning: failed to parse {}: {e}",
                path.display()
            );
            RsxConfig::default()
        }),
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
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    // Retry a few times — app may still be initializing the socket
    for attempt in 0..10 {
        match UnixStream::connect(socket_path) {
            Ok(mut stream) => {
                if let Err(e) = writeln!(stream, "{lib_path}") {
                    eprintln!("[cargo-rsx] Failed to write hot-reload path to socket: {e}");
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
            let status = Command::new("cargo")
                .args(&build_args)
                .env("RSX_HOT_RELOAD_BUILD", "1")
                .env("RUSTFLAGS", &rustflags)
                .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
                .status()
                .expect("[cargo-rsx] failed to invoke cargo");
            if status.success() {
                notify_hot_reload(&socket_path, lib_path.to_str().unwrap_or_default());
                eprintln!("[cargo-rsx] Hot reloaded.");
            } else {
                eprintln!("[cargo-rsx] Build failed, waiting for changes...");
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
    match args.first().map(String::as_str) {
        Some("dev") => run_dev(args[1..].to_vec()),
        Some("bundle") => run_bundle(args[1..].to_vec()),
        Some("preview") => run_preview(args[1..].to_vec()),
        _ => run_passthrough(args),
    }
}

fn run_dev(args: Vec<String>) {
    let (android, rest) = split_android_flag(args);

    if android {
        // `cargo apk run --lib` crashes on UID parsing when launching;
        // work around by doing build → adb install → adb shell am start manually.
        let config = load_config(&rest);
        let mut build_args = vec!["apk".to_string(), "build".to_string(), "--lib".to_string()];
        build_args.extend(rest.iter().cloned());
        inject_feature(&mut build_args, "rsx/dev");

        let status = make_android_cmd(build_args, config)
            .status()
            .expect("[cargo-rsx] failed to invoke cargo");
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }

        android_install_and_launch(&rest);
    } else {
        let config = load_config(&rest);
        let backend_value = backend_str(config.backend.unwrap_or_default());

        let mut cargo_args = vec!["run".to_string()];
        cargo_args.extend(rest.clone());
        inject_feature(&mut cargo_args, "rsx/dev");

        let pkg_dir = find_package_dir(&rest);
        let workspace_root = find_workspace_root(&pkg_dir).unwrap_or(pkg_dir.clone());
        let profile = if rest.contains(&"--release".to_string()) {
            "release"
        } else {
            "debug"
        };

        #[cfg(unix)]
        {
            let pkg_name = read_package_manifest(&rest)
                .map(|p| p.name)
                .unwrap_or_else(|| "app".to_string());
            let lib_path = package_lib_path(&workspace_root, &pkg_name, profile);
            let bin_path = package_bin_path(&workspace_root, &pkg_name, profile);

            // Initial build (produces both binary and dylib)
            let mut build_args = vec!["build".to_string()];
            build_args.extend(rest.clone());
            inject_feature(&mut build_args, "rsx/dev");
            eprintln!("[cargo-rsx] Building...");
            let status = Command::new("cargo")
                .args(&build_args)
                .env("RSX_HOT_RELOAD_BUILD", "1")
                .env("RUSTFLAGS", hot_reload_rustflags())
                .status()
                .expect("[cargo-rsx] failed to invoke cargo");
            if !status.success() {
                eprintln!("[cargo-rsx] Initial build failed. Watching for changes...");
            }

            if bin_path.exists() && lib_path.exists() {
                // Hot reload mode: only rebuild lib on changes
                let lib_build_args = make_lib_build_args(&rest, &["rsx/dev"]);

                let socket_path = format!("/tmp/rsx-hot-{}.sock", std::process::id());
                watch_and_hot_reload(
                    lib_build_args,
                    bin_path,
                    lib_path,
                    socket_path,
                    vec![(
                        "RSX_RENDERER_BACKEND".to_string(),
                        backend_value.to_string(),
                    )],
                    hot_reload_rustflags(),
                    workspace_root,
                );
            }
            // Fallback if binary or lib not found: use process-restart watch
        }

        watch_and_run(
            cargo_args,
            vec![(
                "RSX_RENDERER_BACKEND".to_string(),
                backend_value.to_string(),
            )],
            workspace_root,
        );
    }
}

fn run_preview(args: Vec<String>) {
    let config = load_config(&args);
    let backend_value = backend_str(config.backend.unwrap_or_default());

    let mut cargo_args = vec!["run".to_string()];
    cargo_args.extend(args.clone());
    inject_feature(&mut cargo_args, "rsx/preview");
    inject_feature(&mut cargo_args, "rsx/dev");

    let pkg_dir = find_package_dir(&args);
    let workspace_root = find_workspace_root(&pkg_dir).unwrap_or(pkg_dir.clone());
    let profile = if args.contains(&"--release".to_string()) {
        "release"
    } else {
        "debug"
    };

    let make_envs = || {
        vec![
            (
                "RSX_RENDERER_BACKEND".to_string(),
                backend_value.to_string(),
            ),
            ("RSX_PREVIEW".to_string(), "1".to_string()),
        ]
    };

    #[cfg(unix)]
    {
        let pkg_name = read_package_manifest(&args)
            .map(|p| p.name)
            .unwrap_or_else(|| "app".to_string());
        let lib_path = package_lib_path(&workspace_root, &pkg_name, profile);
        let bin_path = package_bin_path(&workspace_root, &pkg_name, profile);

        let mut build_args = vec!["build".to_string()];
        build_args.extend(args.clone());
        inject_feature(&mut build_args, "rsx/preview");
        inject_feature(&mut build_args, "rsx/dev");
        eprintln!("[cargo-rsx] Building...");
        let status = Command::new("cargo")
            .args(&build_args)
            .env("RSX_HOT_RELOAD_BUILD", "1")
            .env("RSX_PREVIEW_BUILD", "1")
            .env("RUSTFLAGS", preview_rustflags())
            .status()
            .expect("[cargo-rsx] failed to invoke cargo");
        if !status.success() {
            eprintln!("[cargo-rsx] Initial build failed. Watching for changes...");
        }

        if bin_path.exists() && lib_path.exists() {
            let lib_build_args = make_lib_build_args(&args, &["rsx/preview", "rsx/dev"]);

            let mut build_envs = make_envs();
            build_envs.push(("RSX_PREVIEW_BUILD".to_string(), "1".to_string()));
            let socket_path = format!("/tmp/rsx-hot-{}.sock", std::process::id());
            watch_and_hot_reload(
                lib_build_args,
                bin_path,
                lib_path,
                socket_path,
                build_envs,
                preview_rustflags(),
                workspace_root,
            );
        }
        // Fallback if binary or lib not found: restart on every change
    }

    watch_and_run(cargo_args, make_envs(), workspace_root);
}

fn run_bundle(args: Vec<String>) {
    let (android, mut rest) = split_android_flag(args);

    if !rest.contains(&"--release".to_string()) {
        rest.push("--release".to_string());
    }

    if android {
        let config = load_config(&rest);
        let mut build_args = vec!["apk".to_string(), "build".to_string(), "--lib".to_string()];
        build_args.extend(rest);

        let status = make_android_cmd(build_args, config)
            .status()
            .expect("[cargo-rsx] failed to invoke cargo");
        std::process::exit(status.code().unwrap_or(1));
    } else {
        let config = load_config(&rest);
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

fn run_passthrough(args: Vec<String>) {
    let config = load_config(&args);
    let backend_value = backend_str(config.backend.unwrap_or_default());

    let status = Command::new("cargo")
        .args(&args)
        .env("RSX_RENDERER_BACKEND", backend_value)
        .status()
        .expect("[cargo-rsx] failed to invoke cargo");

    std::process::exit(status.code().unwrap_or(1));
}

fn backend_str(backend: RendererBackend) -> &'static str {
    match backend {
        RendererBackend::Auto => "auto",
        RendererBackend::Hardware => "hardware",
        RendererBackend::Software => "software",
    }
}
