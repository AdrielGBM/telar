//! Reading a project's configuration: `telar.toml`, `[package.metadata.telar]` and the manifest fields the bundlers need.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::Deserialize;

pub(crate) use telar_project::{DevSection, TelarSection, WindowSection};
use telar_project::{RendererBackend, TelarManifest};

/// The three words the `--backend` flag takes.
///
/// A CLI vocabulary, not a config one: [`telar_project::RendererBackend`] is the same three words parsed by serde out of `telar.toml`, and this is what clap parses out of an argument. They convert; they are not one type because `ValueEnum` cannot be derived for a type from another crate.
#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum BackendArg {
    Auto,
    Hardware,
    Software,
}

impl From<BackendArg> for RendererBackend {
    fn from(arg: BackendArg) -> Self {
        match arg {
            BackendArg::Auto => Self::Auto,
            BackendArg::Hardware => Self::Hardware,
            BackendArg::Software => Self::Software,
        }
    }
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
    // Only the names are read: what a feature turns on is cargo's business, and all this has to know is whether the package named one.
    #[serde(default)]
    pub(crate) features: BTreeMap<String, toml::Value>,
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
    pub(crate) telar: Option<TelarSection>,
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
        if let Some(root) = telar_project::find_workspace_root(&cwd)
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
    // What the frontend selection reads before deciding it may turn the package's defaults off.
    pub(crate) features: BTreeMap<String, toml::Value>,
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

    /// Whether the package's `default` turns `wanted` on, in either spelling and through however many of its own features it takes to get there.
    fn default_names(&self, wanted: &str) -> bool {
        let enabled = closure(&self.features, feature_list(self.features.get(DEFAULT)));
        enabled.contains(wanted) || enabled.contains(&format!("telar/{wanted}"))
    }

    /// Whether this package runs in the terminal without being told to: its `default` names the terminal frontend and nothing that opens a window.
    ///
    /// What `--target tui` says outright, for a project that already said it in its manifest — and what a `cargo telar dev` there needs to know, or it goes looking for a window to hot-reload behind.
    pub(crate) fn defaults_to_terminal(&self) -> bool {
        self.default_names("tui")
            && !["desktop", "web", "web-dom", "android"]
                .iter()
                .any(|frontend| self.default_names(frontend))
    }

    /// How this package reaches `wanted`, which is the frontend feature a `--target` asked for.
    pub(crate) fn frontend_feature(&self, wanted: &str) -> FrontendFeature {
        if self.default_names(wanted) {
            return FrontendFeature::AlreadyDefault;
        }
        if !self.features.contains_key(wanted) {
            return FrontendFeature::Telar(format!("telar/{wanted}"));
        }
        let name = self.name();
        let mut named = vec![format!("{name}/{wanted}")];
        named.extend(
            feature_list(self.features.get(DEFAULT))
                .into_iter()
                .filter(|feature| !enables_frontend(&self.features, feature))
                .map(|feature| match feature.contains('/') {
                    true => feature,
                    false => format!("{name}/{feature}"),
                }),
        );
        FrontendFeature::Package(named)
    }
}

/// How a build reaches the frontend a `--target` named.
///
/// A project says which frontend it builds in its `default` — that is what `cargo telar new` writes into it — so naming another one means turning that default off. Adding it on top instead is what made `--target tui` compile a desktop stack beside the terminal one: harmless on a machine that can build both, and the whole of the failure on one that cannot.
pub(crate) enum FrontendFeature {
    /// The package's `default` already turns it on, so the build says nothing and keeps every default it has.
    AlreadyDefault,
    /// The package declares a feature for it. The rest of its `default` is re-named alongside, because `--no-default-features` is the only lever cargo has and it takes the whole list — only the *other* frontends in there are dropped.
    Package(Vec<String>),
    /// The package names no such feature, so the dependency's is what turns the frontend on and the defaults stay as they are. What lets any project build for a target without first declaring one.
    Telar(String),
}

impl FrontendFeature {
    /// Appends what cargo needs to build exactly this frontend.
    pub(crate) fn push_to(&self, args: &mut Vec<String>) {
        match self {
            Self::AlreadyDefault => {}
            Self::Package(features) => {
                args.push("--no-default-features".to_string());
                args.push("--features".to_string());
                args.push(features.join(","));
            }
            Self::Telar(feature) => {
                args.push("--features".to_string());
                args.push(feature.clone());
            }
        }
    }
}

const DEFAULT: &str = "default";

/// Every frontend a package can name a feature for, in the spelling `telar` gives it. What a retained default is filtered against, so `--target tui` drops the other targets out of `default` and keeps everything else in it.
const FRONTENDS: [&str; 6] = ["desktop", "tui", "web", "web-dom", "android", "headless"];

fn feature_list(value: Option<&toml::Value>) -> Vec<String> {
    value
        .and_then(toml::Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Every feature `roots` turn on, including the ones reached through another of the package's own — a `default` that names one feature which names a frontend is still a project that chose that frontend.
fn closure(features: &BTreeMap<String, toml::Value>, roots: Vec<String>) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut queue = roots;
    while let Some(feature) = queue.pop() {
        if !seen.insert(feature.clone()) {
            continue;
        }
        queue.extend(feature_list(features.get(&feature)));
    }
    seen
}

fn enables_frontend(features: &BTreeMap<String, toml::Value>, feature: &str) -> bool {
    closure(features, vec![feature.to_string()])
        .iter()
        .any(|enabled| FRONTENDS.contains(&enabled.trim_start_matches("telar/")))
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
    let workspace_root = telar_project::find_workspace_root(&dir).unwrap_or_else(|| dir.clone());
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
    let features = manifest
        .as_ref()
        .map(|m| m.features.clone())
        .unwrap_or_default();
    ResolvedPackage {
        workspace_root,
        package: manifest.and_then(|m| m.package),
        workspace_package,
        produces_cdylib,
        features,
    }
}

// Shared by the Android package id and macOS bundle id defaults, which are otherwise distinct.
pub(crate) fn default_app_id(name: &str) -> String {
    format!("com.example.{name}")
}

// Reads `[package.metadata.telar]` from the package's Cargo.toml (the lowest-precedence file source).
fn read_manifest_config(dir: &Path) -> TelarSection {
    let Ok(content) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
        return TelarSection::default();
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
//
// Fatal rather than a warning, which is what it used to be. The schema refuses a key it does not recognise, and a project that misspelled one has configured nothing — carrying on would run the build the author did not ask for and say so in a line they have already scrolled past.
fn read_toml_config(dir: &Path) -> TelarSection {
    match TelarManifest::load(dir) {
        Ok(manifest) => manifest.telar,
        Err(e) => {
            eprintln!("[cargo-telar] {e}");
            std::process::exit(2);
        }
    }
}

// Lowest to highest: built-in defaults, `[package.metadata.telar]`, `telar.toml`, CLI flags. The flags are layered on by each command after this returns.
pub(crate) fn load_config(args: &[String]) -> TelarSection {
    let dir = find_package_dir(args);
    merge_config(read_manifest_config(&dir), read_toml_config(&dir))
}

fn merge_opt<T>(base: Option<T>, over: Option<T>, merge: impl FnOnce(T, T) -> T) -> Option<T> {
    match (base, over) {
        (Some(b), Some(o)) => Some(merge(b, o)),
        (b, o) => o.or(b),
    }
}

fn merge_window(base: WindowSection, over: WindowSection) -> WindowSection {
    WindowSection {
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

fn merge_dev(base: DevSection, over: DevSection) -> DevSection {
    DevSection {
        window: merge_opt(base.window, over.window, merge_window),
        devtools: over.devtools.or(base.devtools),
    }
}

fn merge_config(base: TelarSection, over: TelarSection) -> TelarSection {
    TelarSection {
        backend: over.backend.or(base.backend),
        dev: merge_dev(base.dev, over.dev),
        // Everything else is read from the package's own `telar.toml` by whoever needs it, never from `[package.metadata.telar]`, so a merge would only invent a precedence nothing reads.
        ..over
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

/// Says so when this binary is not the one the project it is about to build was written against.
///
/// The two halves agree through the artifact format, not through version numbers, so a difference here is usually harmless: same format, compatible output, nothing to do. What it must not be is *invisible*. When the formats do differ the macro refuses with a message naming a command, and the one fact that would explain it — that the installed CLI is not this project's — is the one nobody is told. Hence a note rather than a warning: most of the time it is not the problem.
pub(crate) fn foreign_version_note(project_telar: &str) -> Option<String> {
    let ours = env!("CARGO_PKG_VERSION");
    if project_telar == ours {
        return None;
    }
    Some(format!(
        "[cargo-telar] note: this project builds telar {project_telar}, and this is cargo-telar {ours}. If a build refuses what was transpiled, the versions are why: cargo install cargo-telar --version {project_telar} --force"
    ))
}

/// The same, said once per invocation however many passes ask.
pub(crate) fn warn_if_foreign_version(project_telar: &str) {
    static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if let Some(note) = foreign_version_note(project_telar)
        && !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        eprintln!("{note}");
    }
}

#[cfg(test)]
#[path = "config_test.rs"]
mod tests;
