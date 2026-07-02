use std::path::{Path, PathBuf};

use rsx::RendererBackend;
use serde::Deserialize;

#[derive(Deserialize, Default, Clone)]
pub(crate) struct WindowConfig {
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
pub(crate) struct DevConfig {
    #[serde(default)]
    pub window: Option<WindowConfig>,
    #[serde(default)]
    pub devtools: Option<bool>,
}

#[derive(Deserialize, Default)]
pub(crate) struct RsxConfig {
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
pub(crate) struct CargoWorkspace {
    pub(crate) members: Vec<String>,
}
#[derive(Deserialize, Default)]
pub(crate) struct CargoManifest {
    pub(crate) workspace: Option<CargoWorkspace>,
    pub(crate) package: Option<CargoPackage>,
}
#[derive(Deserialize, Default)]
pub(crate) struct CargoPackage {
    pub(crate) name: String,
    #[serde(default, deserialize_with = "deserialize_inheritable_version")]
    pub(crate) version: Option<String>,
    #[serde(default)]
    pub(crate) description: Option<String>,
    pub(crate) metadata: Option<CargoPackageMetadata>,
}

// `version` may be a plain string or a workspace-inherited table (`version.workspace = true`); accept either shape and keep only a concrete string so parsing never fails.
fn deserialize_inheritable_version<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = toml::Value::deserialize(deserializer)?;
    Ok(value.as_str().map(|s| s.to_owned()))
}
#[derive(Deserialize, Default)]
pub(crate) struct CargoPackageMetadata {
    pub(crate) android: Option<AndroidMetadata>,
    // `[package.metadata.rsx]` — same schema as rsx.toml's `[rsx]`, but overridden by rsx.toml.
    pub(crate) rsx: Option<RsxConfig>,
}
#[derive(Deserialize, Default)]
pub(crate) struct AndroidMetadata {
    pub(crate) package: Option<String>,
}

pub(crate) fn expand_member(workspace_root: &Path, pattern: &str) -> Vec<PathBuf> {
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

pub(crate) fn find_package_dir(args: &[String]) -> PathBuf {
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

pub(crate) fn read_package_manifest(args: &[String]) -> Option<CargoPackage> {
    let dir = find_package_dir(args);
    let content = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    let manifest: CargoManifest = toml::from_str(&content).ok()?;
    manifest.package
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
pub(crate) fn load_config(args: &[String]) -> RsxConfig {
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

pub(crate) fn backend_as_str(backend: RendererBackend) -> &'static str {
    match backend {
        RendererBackend::Auto => "auto",
        RendererBackend::Hardware => "hardware",
        RendererBackend::Software => "software",
    }
}

pub(crate) fn split_android_flag(args: Vec<String>) -> (bool, Vec<String>) {
    let android = args.contains(&"--android".to_string());
    let rest = args.into_iter().filter(|a| a != "--android").collect();
    (android, rest)
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

    #[test]
    fn workspace_inherited_version_deserializes_to_none() {
        // `version.workspace = true` must not fail parsing; it yields no concrete string.
        let manifest: CargoManifest = toml::from_str(
            "[package]\nname = \"demo\"\nversion.workspace = true\ndescription = \"hi\"\n",
        )
        .expect("workspace-inherited version should parse");
        let pkg = manifest.package.expect("package section");
        assert_eq!(pkg.name, "demo");
        assert_eq!(pkg.version, None);
        assert_eq!(pkg.description.as_deref(), Some("hi"));
    }

    #[test]
    fn explicit_version_string_deserializes() {
        let manifest: CargoManifest =
            toml::from_str("[package]\nname = \"demo\"\nversion = \"2.0.1\"\n")
                .expect("explicit version should parse");
        let pkg = manifest.package.expect("package section");
        assert_eq!(pkg.version.as_deref(), Some("2.0.1"));
    }
}
