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

pub fn run(args: Vec<String>) {
    match args.first().map(String::as_str) {
        Some("dev") => run_dev(args[1..].to_vec()),
        _ => run_passthrough(args),
    }
}

fn inject_dev_feature(args: &mut Vec<String>) {
    if let Some(pos) = args.iter().position(|a| a == "--features" || a == "-F") {
        if pos + 1 < args.len() {
            args[pos + 1] = format!("{},rsx/dev", args[pos + 1]);
            return;
        }
    }
    args.push("--features".to_string());
    args.push("rsx/dev".to_string());
}

fn run_dev(args: Vec<String>) {
    let config = load_config(&args);
    let backend_value = backend_str(config.backend.unwrap_or_default());

    let mut cargo_args = Vec::with_capacity(args.len() + 3);
    cargo_args.push("run".to_string());
    cargo_args.extend(args);
    inject_dev_feature(&mut cargo_args);

    let status = Command::new("cargo")
        .args(&cargo_args)
        .env("RSX_RENDERER_BACKEND", backend_value)
        .status()
        .expect("[cargo-rsx] failed to invoke cargo");

    std::process::exit(status.code().unwrap_or(1));
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
