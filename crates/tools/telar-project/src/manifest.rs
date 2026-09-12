//! `telar.toml`, as one schema.
//!
//! It was read in six places with six partial shapes: a raw `toml::Table` for `assets`, an ad-hoc lookup for `theme`, a third for `[telar.i18n]` and its two back-compat spellings, and a serde struct in the CLI that knew only `backend` and `dev`. Every one of them treated a key it did not recognise as absent, so nothing in the project could tell a setting that was off from a setting that was misspelled.
//!
//! That is not hypothetical. `cargo-telar`'s struct carries a comment recording the time it happened: the table was renamed `rsx` → `telar`, its reader kept looking for the old name, and because the field was `#[serde(default)]` a file writing `[telar]` parsed clean with every key in it ignored.
//!
//! So: one struct, `deny_unknown_fields` at **every** level, and a [`load`](TelarManifest::load) that says which key it could not place.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The file this describes, in the package root.
pub const MANIFEST_FILENAME: &str = "telar.toml";

/// Which renderer the built application selects at startup.
///
/// The *config* vocabulary. A CLI flag offering the same three words is a different thing with the same spelling — it is parsed by a different library, and it can be given where no manifest exists.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum RendererBackend {
    /// Hardware, falling back to software where there is no adapter.
    #[default]
    Auto,
    Hardware,
    Software,
}

impl RendererBackend {
    /// The spelling the runtime re-parses out of `TELAR_RENDERER_BACKEND`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Hardware => "hardware",
            Self::Software => "software",
        }
    }
}

/// The dev-session settings: the window `cargo telar dev` opens, and whether it draws the overlay.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DevSection {
    #[serde(default)]
    pub window: Option<WindowSection>,
    #[serde(default)]
    pub devtools: Option<bool>,
}

/// The window a dev session opens, overriding what the application configured.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WindowSection {
    pub title: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub decorations: Option<bool>,
    pub resizable: Option<bool>,
    pub transparent: Option<bool>,
    /// `"disabled"`, `"borderless"` or `"exclusive"`.
    pub fullscreen: Option<String>,
    /// `"centered"`, or `"<x>,<y>"`.
    pub position: Option<String>,
}

/// Where translation catalogs are discovered. Both sources may be active at once; either name set to `""` disables it.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct I18nSection {
    /// The project-wide catalog directory, joined onto the package root. Default `"locales"`.
    pub root: Option<String>,
    /// The directory name looked for recursively under `src/`, for catalogs kept beside the module that reads them. Default `"i18n"`.
    pub scan: Option<String>,
    /// The locale a lookup falls back to. `None` means `"en"` if present, else the first tag found.
    pub default: Option<String>,
}

/// The `[telar]` table.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TelarSection {
    pub backend: Option<RendererBackend>,
    /// The directory a baked `src:"…"` resolves against, joined onto the package root. Default `"assets"`.
    pub assets: Option<String>,
    /// The theme type this package's components resolve `use_theme` against.
    pub theme: Option<String>,
    #[serde(default)]
    pub dev: DevSection,
    #[serde(default)]
    pub i18n: I18nSection,
    /// The pre-`[telar.i18n]` spelling of [`I18nSection::root`].
    pub locales: Option<String>,
    /// The pre-`[telar.i18n]` spelling of [`I18nSection::default`].
    pub default_locale: Option<String>,
}

/// The whole file.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TelarManifest {
    #[serde(default)]
    pub telar: TelarSection,
}

/// Why a `telar.toml` could not be read.
#[derive(Debug)]
pub enum ManifestError {
    /// The file is there and does not parse, or names something the schema has no place for. The message is serde's, which names the key and the line.
    Invalid { path: PathBuf, message: String },
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid { path, message } => write!(f, "{}: {message}", path.display()),
        }
    }
}

impl std::error::Error for ManifestError {}

impl TelarManifest {
    /// Reads `<package_root>/telar.toml`, over the workspace root's if there is one.
    ///
    /// A package with no manifest gets the defaults — every setting here has one, and a project that configures nothing is the common case. A manifest that *exists* and cannot be understood is an error, which is the whole point: the alternative is what this replaces, where an unreadable file and a misspelled key both read as "not configured".
    ///
    /// Settings are inherited key by key, so a workspace declares its theme, backend and catalogs once and a package overrides only what differs from its siblings. Every package of a workspace answering the same way is the common case, and saying it eight times is how the eight drift apart.
    pub fn load(package_root: &Path) -> Result<Self, ManifestError> {
        let own = Self::read(package_root)?;
        let inherited = crate::find_workspace_root(package_root)
            .filter(|root| root != package_root)
            .map(|root| Self::read(&root))
            .transpose()?
            .flatten();
        Ok(match (own, inherited) {
            (Some(own), Some(base)) => Self {
                telar: own.telar.over(base.telar),
            },
            (own, base) => own.or(base).unwrap_or_default(),
        })
    }

    fn read(dir: &Path) -> Result<Option<Self>, ManifestError> {
        let path = dir.join(MANIFEST_FILENAME);
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Ok(None);
        };
        toml::from_str(&content)
            .map(Some)
            .map_err(|e| ManifestError::Invalid {
                path,
                message: e.to_string(),
            })
    }

    /// The same, for a caller with nowhere to report: an unreadable manifest yields the defaults.
    ///
    /// Only for a path that genuinely cannot surface an error. Prefer [`load`](Self::load) — a misspelled key that silently does nothing is the failure this module exists to end.
    pub fn load_or_default(package_root: &Path) -> Self {
        Self::load(package_root).unwrap_or_default()
    }
}

impl TelarSection {
    /// This table's own keys, falling back to `base` key by key.
    fn over(self, base: Self) -> Self {
        Self {
            backend: self.backend.or(base.backend),
            assets: self.assets.or(base.assets),
            theme: self.theme.or(base.theme),
            dev: DevSection {
                window: self.dev.window.or(base.dev.window),
                devtools: self.dev.devtools.or(base.dev.devtools),
            },
            i18n: I18nSection {
                root: self.i18n.root.or(base.i18n.root),
                scan: self.i18n.scan.or(base.i18n.scan),
                default: self.i18n.default.or(base.i18n.default),
            },
            locales: self.locales.or(base.locales),
            default_locale: self.default_locale.or(base.default_locale),
        }
    }

    /// The directory a baked `src:"…"` resolves against, joined onto `package_root`.
    pub fn assets_root(&self, package_root: &Path) -> PathBuf {
        package_root.join(self.assets.as_deref().unwrap_or("assets"))
    }

    /// The project-wide catalog directory, or `None` when it is disabled with `""`.
    pub fn locales_root(&self, package_root: &Path) -> Option<PathBuf> {
        let root = self
            .i18n
            .root
            .clone()
            .or_else(|| self.locales.clone())
            .unwrap_or_else(|| "locales".to_string());
        (!root.is_empty()).then(|| package_root.join(root))
    }

    /// The directory name looked for under `src/` for co-located catalogs.
    pub fn catalog_scan_dir(&self) -> String {
        self.i18n.scan.clone().unwrap_or_else(|| "i18n".to_string())
    }

    /// The locale a lookup falls back to, if the project names one.
    pub fn default_locale(&self) -> Option<String> {
        self.i18n
            .default
            .clone()
            .or_else(|| self.default_locale.clone())
    }
}

#[cfg(test)]
#[path = "manifest_test.rs"]
mod tests;
