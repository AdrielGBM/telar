//! Reading a project's configuration: `telar.toml`, `[package.metadata.telar]` and the manifest fields the bundlers need.

use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::Deserialize;

/// Which renderer the built app selects at startup. Declared here rather than imported from `telar` because the value never crosses the boundary: [`backend_as_str`] lowers it to the `TELAR_RENDERER_BACKEND` env var, which the runtime re-parses from `option_env!` — so importing it cost the CLI the whole facade.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RendererBackend {
    #[default]
    Auto,
    Hardware,
    Software,
}

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

#[derive(Deserialize, Default, Clone)]
pub(crate) struct TelarConfig {
    #[serde(default)]
    pub backend: Option<RendererBackend>,
    #[serde(default)]
    pub dev: Option<DevConfig>,
    // `[telar] auto_modules` is read directly by the `telar::app!` macro, and serde ignores the unknown key here.
}

// The field name is the table name in telar.toml. It was `rsx` before the rename and nothing caught it: the table is `#[serde(default)]`, so a file writing `[telar]` parsed clean and every key in it was ignored.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct TelarToml {
    #[serde(default)]
    pub telar: TelarConfig,
}

#[derive(Deserialize, Default)]
pub(crate) struct CargoWorkspace {
    #[serde(default)]
    pub(crate) members: Vec<String>,
    pub(crate) package: Option<CargoWorkspacePackage>,
}
// `[workspace.package]` — the values a member inherits with `field.workspace = true`.
#[derive(Deserialize, Default, Clone)]
pub(crate) struct CargoWorkspacePackage {
    #[serde(default)]
    pub(crate) version: Option<String>,
    #[serde(default)]
    pub(crate) authors: Vec<String>,
    #[serde(default)]
    pub(crate) description: Option<String>,
}
#[derive(Deserialize, Default)]
pub(crate) struct CargoManifest {
    pub(crate) workspace: Option<CargoWorkspace>,
    pub(crate) package: Option<CargoPackage>,
    pub(crate) lib: Option<CargoLib>,
}
#[derive(Deserialize, Default)]
pub(crate) struct CargoLib {
    #[serde(default, rename = "crate-type")]
    pub(crate) crate_type: Vec<String>,
}
#[derive(Deserialize, Default)]
pub(crate) struct CargoPackage {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) version: Inheritable<String>,
    #[serde(default)]
    pub(crate) authors: Inheritable<Vec<String>>,
    #[serde(default)]
    pub(crate) description: Inheritable<String>,
    pub(crate) metadata: Option<CargoPackageMetadata>,
}

// Inherited is kept apart from absent because only the inherited case has an answer in the workspace manifest; collapsing the two let a member's real version reach a `.deb` as the hardcoded fallback.
#[derive(Default, Debug, PartialEq)]
pub(crate) enum Inheritable<T> {
    #[default]
    Absent,
    FromWorkspace,
    Set(T),
}

impl<'de, T: serde::de::DeserializeOwned> Deserialize<'de> for Inheritable<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = toml::Value::deserialize(deserializer)?;
        if value.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
            return Ok(Self::FromWorkspace);
        }
        T::deserialize(value)
            .map(Self::Set)
            .map_err(serde::de::Error::custom)
    }
}

impl<T: Clone> Inheritable<T> {
    fn resolve(&self, inherited: impl FnOnce() -> Option<T>) -> Option<T> {
        match self {
            Self::Set(value) => Some(value.clone()),
            Self::FromWorkspace => inherited(),
            Self::Absent => None,
        }
    }
}
#[derive(Deserialize, Default)]
pub(crate) struct CargoPackageMetadata {
    pub(crate) android: Option<AndroidMetadata>,
    // `[package.metadata.telar]` — same schema as telar.toml's `[telar]`, but overridden by telar.toml.
    pub(crate) telar: Option<TelarConfig>,
}
#[derive(Deserialize, Default)]
pub(crate) struct AndroidMetadata {
    pub(crate) package: Option<String>,
}

// A `*` in any segment, not just a trailing one: a workspace nesting its members two levels deep expanded to nothing, leaving every crate under it unwatched and unfindable by name.
pub(crate) fn expand_member(workspace_root: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut paths = vec![workspace_root.to_path_buf()];
    for segment in pattern.split('/') {
        paths = if segment == "*" {
            paths
                .iter()
                .filter_map(|path| std::fs::read_dir(path).ok())
                .flatten()
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        } else {
            paths.into_iter().map(|path| path.join(segment)).collect()
        };
    }
    paths
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
        if let Some(root) = telar_transpiler::find_workspace_root(&cwd)
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

fn read_manifest_in(dir: &Path) -> Option<CargoManifest> {
    let content = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    toml::from_str(&content).ok()
}

fn read_package_manifest_in(dir: &Path) -> Option<CargoPackage> {
    read_manifest_in(dir)?.package
}

pub(crate) fn read_package_manifest(args: &[String]) -> Option<CargoPackage> {
    read_package_manifest_in(&find_package_dir(args))
}

pub(crate) struct ResolvedPackage {
    pub(crate) workspace_root: PathBuf,
    pub(crate) package: Option<CargoPackage>,
    // Read once here rather than per getter, since a member that inherits one field usually inherits several.
    pub(crate) workspace_package: Option<CargoWorkspacePackage>,
    // Hot reload dlopens the package's own cdylib, and without `crate-type = ["cdylib", ..]` cargo never emits one, so the dylib build is dead weight and the runner falls back to process restart.
    pub(crate) produces_cdylib: bool,
}

impl ResolvedPackage {
    // Falls back to cargo's default "app" binary name when the manifest can't be read.
    pub(crate) fn name(&self) -> String {
        self.package
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "app".to_string())
    }

    pub(crate) fn version(&self) -> String {
        self.package
            .as_ref()
            .and_then(|p| {
                p.version
                    .resolve(|| self.workspace_package.as_ref()?.version.clone())
            })
            .unwrap_or_else(|| "0.1.0".to_string())
    }

    // Debian's `Maintainer` and Cargo's `authors` share the `Name <email>` shape, so the manifest is the one source. There is no honest default: a placeholder address ships inside the package.
    pub(crate) fn maintainer(&self) -> Option<String> {
        self.package
            .as_ref()
            .and_then(|p| {
                p.authors
                    .resolve(|| Some(self.workspace_package.as_ref()?.authors.clone()))
            })
            .and_then(|authors| authors.first().cloned())
            .or_else(maintainer_from_env)
    }

    pub(crate) fn description(&self) -> Option<String> {
        self.package.as_ref().and_then(|p| {
            p.description
                .resolve(|| self.workspace_package.as_ref()?.description.clone())
        })
    }
}

// dpkg reads the maintainer from `DEBFULLNAME`/`DEBEMAIL`, so honour the same pair: cargo stopped emitting `authors` years ago, and refusing every manifest without it would rule out most projects.
fn maintainer_from_env() -> Option<String> {
    maintainer_from(
        std::env::var("DEBFULLNAME").ok(),
        std::env::var("DEBEMAIL").ok(),
    )
}

fn maintainer_from(name: Option<String>, email: Option<String>) -> Option<String> {
    let email = email?;
    let email = email.trim();
    if email.is_empty() {
        return None;
    }
    match name {
        Some(name) if !name.trim().is_empty() => Some(format!("{} <{email}>", name.trim())),
        _ => Some(email.to_string()),
    }
}

// Resolved in a single pass, so the packaging paths stop re-deriving them at each call site.
pub(crate) fn resolve_package(args: &[String]) -> ResolvedPackage {
    let dir = find_package_dir(args);
    let workspace_root = telar_transpiler::find_workspace_root(&dir).unwrap_or_else(|| dir.clone());
    let manifest = read_manifest_in(&dir);
    let produces_cdylib = manifest
        .as_ref()
        .and_then(|m| m.lib.as_ref())
        .is_some_and(|lib| lib.crate_type.iter().any(|kind| kind == "cdylib"));
    // The member's own manifest when it is also the workspace root, so a single-crate project inherits from itself without a second read.
    let workspace_package = if workspace_root == dir {
        manifest.as_ref()
    } else {
        None
    }
    .and_then(|m| m.workspace.as_ref())
    .and_then(|w| w.package.clone())
    .or_else(|| read_manifest_in(&workspace_root)?.workspace?.package);
    ResolvedPackage {
        workspace_root,
        package: manifest.and_then(|m| m.package),
        workspace_package,
        produces_cdylib,
    }
}

// Shared by the Android package id and macOS bundle id defaults, which are otherwise distinct.
pub(crate) fn default_app_id(name: &str) -> String {
    format!("com.example.{name}")
}

// Reads `[package.metadata.telar]` from the package's Cargo.toml (the lowest-precedence file source).
fn read_manifest_config(dir: &Path) -> TelarConfig {
    let Ok(content) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
        return TelarConfig::default();
    };
    toml::from_str::<CargoManifest>(&content)
        .ok()
        .and_then(|m| m.package)
        .and_then(|p| p.metadata)
        .and_then(|m| m.telar)
        .unwrap_or_default()
}

// `read_manifest_config` erases the present/absent distinction via `unwrap_or_default`, so presence needs its own read.
pub(crate) fn manifest_has_telar(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("Cargo.toml"))
        .ok()
        .and_then(|c| toml::from_str::<CargoManifest>(&c).ok())
        .and_then(|m| m.package)
        .and_then(|p| p.metadata)
        .and_then(|m| m.telar)
        .is_some()
}

// Reads `[telar]` from telar.toml, which overrides the manifest metadata.
fn read_toml_config(dir: &Path) -> TelarConfig {
    let path = dir.join("telar.toml");
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            toml::from_str::<TelarToml>(&content)
                .unwrap_or_else(|e| {
                    eprintln!(
                        "[cargo-telar] Warning: failed to parse {}: {e}",
                        path.display()
                    );
                    TelarToml::default()
                })
                .telar
        }
        Err(_) => TelarConfig::default(),
    }
}

// Lowest to highest: built-in defaults, `[package.metadata.telar]`, `telar.toml`, CLI flags. The flags are layered on by each command after this returns.
pub(crate) fn load_config(args: &[String]) -> TelarConfig {
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

fn merge_config(base: TelarConfig, over: TelarConfig) -> TelarConfig {
    TelarConfig {
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
    fn telar_toml_overrides_manifest_field_by_field() {
        let manifest = TelarConfig {
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
        let file = TelarConfig {
            backend: Some(RendererBackend::Hardware),
            dev: Some(DevConfig {
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
        assert_eq!(dev.devtools, Some(true)); // omitted in telar.toml → manifest value survives
        let window = dev.window.unwrap();
        assert_eq!(window.width, Some(1024)); // telar.toml wins
        assert_eq!(window.height, Some(600)); // falls back to manifest
        assert_eq!(window.position.as_deref(), Some("10,20")); // only in telar.toml
        assert_eq!(window.fullscreen.as_deref(), Some("disabled")); // only in manifest
    }

    #[test]
    fn telar_table_is_the_one_read_from_telar_toml() {
        let parsed: TelarToml =
            toml::from_str("[telar]\nbackend = \"software\"\nauto_modules = true\n")
                .expect("[telar] should parse, and `auto_modules` is the app macro's to read");
        assert!(matches!(
            parsed.telar.backend,
            Some(RendererBackend::Software)
        ));
    }

    // The rename that named this table `telar` left the field called `rsx`, and a `#[serde(default)]` table means a file writing `[telar]` still parsed clean — backend, dev window and devtools dropped in silence.
    #[test]
    fn a_top_level_table_that_is_not_telar_is_rejected_rather_than_ignored() {
        assert!(toml::from_str::<TelarToml>("[rsx]\nbackend = \"software\"\n").is_err());
    }

    #[test]
    fn manifest_used_when_telar_toml_absent() {
        let manifest = TelarConfig {
            backend: Some(RendererBackend::Software),
            dev: None,
        };
        let merged = merge_config(manifest, TelarConfig::default());
        assert!(matches!(merged.backend, Some(RendererBackend::Software)));
        assert!(merged.dev.is_none());
    }

    #[test]
    fn workspace_inherited_version_is_recorded_as_inherited() {
        // `version = { workspace = true }` must not fail parsing; it records that the workspace holds the answer.
        let manifest: CargoManifest = toml::from_str(
            "[package]\nname = \"demo\"\nversion = { workspace = true }\ndescription = \"hi\"\n",
        )
        .expect("workspace-inherited version should parse");
        let pkg = manifest.package.expect("package section");
        assert_eq!(pkg.name, "demo");
        assert_eq!(pkg.version, Inheritable::FromWorkspace);
        assert_eq!(pkg.description, Inheritable::Set("hi".to_string()));
    }

    #[test]
    fn crate_type_distinguishes_a_hot_reloadable_package() {
        let plain: CargoManifest =
            toml::from_str("[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[lib]\n")
                .expect("bare [lib] should parse");
        assert!(plain.lib.expect("lib section").crate_type.is_empty());

        let dylib: CargoManifest = toml::from_str(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[lib]\ncrate-type = [\"cdylib\", \"lib\"]\n",
        )
        .expect("crate-type should parse");
        assert!(
            dylib
                .lib
                .expect("lib section")
                .crate_type
                .iter()
                .any(|kind| kind == "cdylib")
        );
    }

    #[test]
    fn workspace_inherited_authors_are_recorded_as_inherited() {
        let manifest: CargoManifest =
            toml::from_str("[package]\nname = \"demo\"\nauthors = { workspace = true }\n")
                .expect("workspace-inherited authors should parse");
        let pkg = manifest.package.expect("package section");
        assert_eq!(pkg.authors, Inheritable::FromWorkspace);
    }

    #[test]
    fn the_dotted_spelling_of_inheritance_is_read_the_same_way() {
        // `version.workspace = true` is the form cargo's own docs use, and the one hyprshell writes.
        let manifest: CargoManifest =
            toml::from_str("[package]\nname = \"demo\"\nversion.workspace = true\n")
                .expect("dotted inheritance should parse");
        let pkg = manifest.package.expect("package section");
        assert_eq!(pkg.version, Inheritable::FromWorkspace);
    }

    #[test]
    fn explicit_authors_list_deserializes() {
        let manifest: CargoManifest = toml::from_str(
            "[package]\nname = \"demo\"\nauthors = [\"Ada <ada@example.org>\", \"Bo <bo@example.org>\"]\n",
        )
        .expect("explicit authors should parse");
        let pkg = manifest.package.expect("package section");
        assert_eq!(
            pkg.authors,
            Inheritable::Set(vec![
                "Ada <ada@example.org>".to_string(),
                "Bo <bo@example.org>".to_string(),
            ])
        );
    }

    #[test]
    fn explicit_version_string_deserializes() {
        let manifest: CargoManifest =
            toml::from_str("[package]\nname = \"demo\"\nversion = \"2.0.1\"\n")
                .expect("explicit version should parse");
        let pkg = manifest.package.expect("package section");
        assert_eq!(pkg.version, Inheritable::Set("2.0.1".to_string()));
    }

    #[test]
    fn an_inherited_field_is_answered_by_the_workspace_not_the_fallback() {
        // Regression: a member inheriting its version reported the hardcoded "0.1.0", so every bundle carried the wrong version the moment the workspace moved off it.
        let manifest: CargoManifest = toml::from_str(
            "[package]\nname = \"demo\"\nversion.workspace = true\nauthors.workspace = true\n",
        )
        .expect("inherited fields should parse");
        let resolved = ResolvedPackage {
            workspace_root: PathBuf::from("/nowhere"),
            package: manifest.package,
            workspace_package: Some(CargoWorkspacePackage {
                version: Some("2.0.0".to_string()),
                authors: vec!["Ada <ada@example.org>".to_string()],
                description: None,
            }),
            produces_cdylib: false,
        };
        assert_eq!(resolved.version(), "2.0.0");
        assert_eq!(
            resolved.maintainer().as_deref(),
            Some("Ada <ada@example.org>")
        );
    }

    #[test]
    fn the_debian_environment_names_a_maintainer_when_the_manifest_does_not() {
        assert_eq!(
            maintainer_from(Some("Ada".into()), Some("ada@example.org".into())).as_deref(),
            Some("Ada <ada@example.org>")
        );
        // DEBEMAIL alone is a complete answer: dpkg accepts a bare address as the maintainer.
        assert_eq!(
            maintainer_from(None, Some("ada@example.org".into())).as_deref(),
            Some("ada@example.org")
        );
        assert_eq!(maintainer_from(Some("Ada".into()), None), None);
        assert_eq!(maintainer_from(None, Some("   ".into())), None);
    }

    #[test]
    fn an_unreadable_manifest_still_falls_back() {
        let resolved = ResolvedPackage {
            workspace_root: PathBuf::from("/nowhere"),
            package: None,
            workspace_package: None,
            produces_cdylib: false,
        };
        assert_eq!(resolved.name(), "app");
        assert_eq!(resolved.version(), "0.1.0");
        assert_eq!(resolved.maintainer(), None);
    }
}
