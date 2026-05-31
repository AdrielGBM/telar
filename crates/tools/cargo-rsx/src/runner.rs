use std::path::{Path, PathBuf};
use std::process::Command;

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

fn make_android_cmd(cargo_args: Vec<String>) -> Command {
    let ndk_root = resolve_ndk_root();
    let mut cmd = Command::new("cargo");
    cmd.args(cargo_args).env("RSX_RENDERER_BACKEND", "software");
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

pub fn run(args: Vec<String>) {
    match args.first().map(String::as_str) {
        Some("dev") => run_dev(args[1..].to_vec()),
        Some("bundle") => run_bundle(args[1..].to_vec()),
        _ => run_passthrough(args),
    }
}

fn run_dev(args: Vec<String>) {
    let (android, rest) = split_android_flag(args);

    if android {
        // `cargo apk run --lib` crashes on UID parsing when launching;
        // work around by doing build → adb install → adb shell am start manually.
        let mut build_args = vec!["apk".to_string(), "build".to_string(), "--lib".to_string()];
        build_args.extend(rest.iter().cloned());
        inject_feature(&mut build_args, "rsx/dev");

        let status = make_android_cmd(build_args)
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
        cargo_args.extend(rest);
        inject_feature(&mut cargo_args, "rsx/dev");

        let status = Command::new("cargo")
            .args(&cargo_args)
            .env("RSX_RENDERER_BACKEND", backend_value)
            .status()
            .expect("[cargo-rsx] failed to invoke cargo");

        std::process::exit(status.code().unwrap_or(1));
    }
}

fn run_bundle(args: Vec<String>) {
    let (android, mut rest) = split_android_flag(args);

    if !rest.contains(&"--release".to_string()) {
        rest.push("--release".to_string());
    }

    if android {
        let mut build_args = vec!["apk".to_string(), "build".to_string(), "--lib".to_string()];
        build_args.extend(rest);

        let status = make_android_cmd(build_args)
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
