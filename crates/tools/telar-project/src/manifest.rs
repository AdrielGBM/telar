//! `telar.toml`, as one schema.
//!
//! It was read in six places with six partial shapes: a raw `toml::Table` for `assets`, an ad-hoc lookup for `theme`, a third for `[telar.i18n]` and its two back-compat spellings, and a serde struct in the CLI that knew only `backend` and `dev`. Every one of them treated a key it did not recognise as absent, so nothing in the project could tell a setting that was off from a setting that was misspelled.
//!
//! That is not hypothetical. `cargo-telar`'s struct carries a comment recording the time it happened: the table was renamed `rsx` → `telar`, its reader kept looking for the old name, and because the field was `#[serde(default)]` a file writing `[telar]` parsed clean with every key in it ignored.
//!
//! So: one struct, `deny_unknown_fields` at **every** level, and a [`load`](TelarManifest::load) that says which key it could not place.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::fonts::FontDeclaration;
use crate::prelude::PreludeEntry;
use crate::web::{OgImage, ThemeColor, WebHost};

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

/// What only means something to a browser build: `[telar.web]`, read by `cargo telar build --target web` and `dev --target web` and by nothing else.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WebSection {
    /// The page template, joined onto the package root. Default `"web/index.html"`, falling back to the built-in page when that file does not exist.
    pub template: Option<String>,
    /// The directory copied verbatim into the output, joined onto the package root. Default `"web/public"`.
    pub public: Option<String>,
    /// The scheme and host the site is served at (`"https://example.com"`): what canonical links, alternate-language links, Open Graph URLs and the sitemap are written against.
    pub origin: Option<String>,
    /// The path the site is served under. Default `"/"`.
    pub base: Option<String>,
    /// The catalog key a page's description is read from, in the page's own locale.
    pub description: Option<String>,
    /// The picture a link preview shows: a file of the site or a full URL, for every locale or per locale.
    pub og_image: Option<OgImage>,
    /// The color a browser paints its interface in before the app has drawn.
    pub theme_color: Option<ThemeColor>,
    /// The host the output is shaped for. Default `"static"`.
    pub host: Option<WebHost>,
    /// What `cargo telar build --target web --prerender` builds each page under.
    #[serde(default)]
    pub prerender: PrerenderSection,
}

impl WebSection {
    const DEFAULT_TEMPLATE: &str = "web/index.html";
    const DEFAULT_PUBLIC: &str = "web/public";

    /// The template to expand, and whether the project named it rather than leaving the default.
    pub fn template_path(&self, package_root: &Path) -> (PathBuf, bool) {
        Self::resolve(
            package_root,
            self.template.as_deref(),
            Self::DEFAULT_TEMPLATE,
        )
    }

    /// The directory to copy, and whether the project named it rather than leaving the default.
    pub fn public_dir(&self, package_root: &Path) -> (PathBuf, bool) {
        Self::resolve(package_root, self.public.as_deref(), Self::DEFAULT_PUBLIC)
    }

    fn resolve(package_root: &Path, named: Option<&str>, default: &str) -> (PathBuf, bool) {
        (package_root.join(named.unwrap_or(default)), named.is_some())
    }
}

/// `[telar.web.prerender]`: the reader a page written ahead of time is written for, before anybody has asked the browser. Each value left out is one the page is built not knowing, as a browser that has not answered yet would report it.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PrerenderSection {
    /// The surface width in CSS pixels. Default 1280.
    pub width: Option<u32>,
    /// The surface height in CSS pixels. Default 800.
    pub height: Option<u32>,
    /// `"light"` or `"dark"`.
    pub color_scheme: Option<String>,
    pub reduced_motion: Option<bool>,
    pub high_contrast: Option<bool>,
    /// The system locales assumed, most preferred first.
    #[serde(default)]
    pub locales: Vec<String>,
}

impl PrerenderSection {
    /// The surface a page is laid out on.
    pub fn surface(&self) -> crate::Surface {
        let fallback = crate::Surface::default();
        crate::Surface {
            width: self.width.unwrap_or(fallback.width),
            height: self.height.unwrap_or(fallback.height),
        }
    }

    fn over(self, base: Self) -> Self {
        Self {
            width: self.width.or(base.width),
            height: self.height.or(base.height),
            color_scheme: self.color_scheme.or(base.color_scheme),
            reduced_motion: self.reduced_motion.or(base.reduced_motion),
            high_contrast: self.high_contrast.or(base.high_contrast),
            locales: if self.locales.is_empty() {
                base.locales
            } else {
                self.locales
            },
        }
    }

    /// The preferences a page is built under.
    pub fn preferences(&self) -> crate::Preferences {
        crate::Preferences {
            color_scheme: self.color_scheme.clone(),
            reduced_motion: self.reduced_motion,
            high_contrast: self.high_contrast,
            locales: self.locales.clone(),
        }
    }
}

/// The `[telar]` table.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TelarSection {
    pub backend: Option<RendererBackend>,
    /// The directory a baked `src:"…"` resolves against, joined onto the package root. Default `"assets"`.
    pub assets: Option<String>,
    /// The theme type this package's components resolve `use_theme` against. A library may not set it, since it cannot name the theme type of an application that depends on it.
    pub theme: Option<String>,
    /// The crates whose items every `.rsx` of this package can name as tags, glob-imported after `telar`'s and before the package's own. `None` inherits the workspace's, except in a library, and `[]` declares none, so a package can opt out of a crate its workspace shares. Read through [`Self::prelude`] or [`crate::resolve_prelude`].
    pub prelude: Option<Vec<PreludeEntry>>,
    /// Whether this package is a library other crates depend on, such as a plugin written in `.rsx`. Compiled as a dependency, its `.rsx` is wired read-only from the Plain artifact it ships, and its `t!` lets the application override each string under the package's name.
    ///
    /// Never inherited: a workspace sharing one `telar.toml` holds applications too, and what a package is cannot be a default. Nor does a library inherit anything else; see [`TelarManifest::load`].
    #[serde(default)]
    pub library: bool,
    #[serde(default)]
    pub dev: DevSection,
    #[serde(default)]
    pub i18n: I18nSection,
    #[serde(default)]
    pub web: WebSection,
    /// The faces this project ships, one entry per file. See [`FontDeclaration`].
    #[serde(default)]
    pub fonts: Vec<FontDeclaration>,
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
    ///
    /// A `[telar] library` inherits nothing: it ships its artifact transpiled against its own manifest, and once published there is no workspace above it, or there is an application's, when a copy is vendored into one. Either way the answer would differ from the one the artifact was written under.
    pub fn load(package_root: &Path) -> Result<Self, ManifestError> {
        let own = Self::read(package_root)?;
        let library = own.as_ref().is_some_and(|own| own.telar.library);
        let inherited = Self::workspace_dir(package_root, library)
            .map(|root| Self::read(&root))
            .transpose()?
            .flatten();
        let mut manifest = match (own, inherited) {
            (Some(own), Some(base)) => Self {
                telar: own.telar.over(base.telar),
            },
            (own, base) => own.or(base).unwrap_or_default(),
        };
        manifest.telar.library = library;
        let problems = manifest.telar.web.problems();
        if !problems.is_empty() {
            return Err(ManifestError::Invalid {
                path: package_root.join(MANIFEST_FILENAME),
                message: problems.join("; "),
            });
        }
        Ok(manifest)
    }

    /// Every `telar.toml` [`load`](Self::load) reads for `package_root` that exists: the package's own, then the workspace's it inherits from, which a library has none of. What a build has to track to notice an inherited setting changing.
    pub fn files(package_root: &Path) -> Vec<PathBuf> {
        let library = Self::read(package_root)
            .ok()
            .flatten()
            .is_some_and(|own| own.telar.library);
        let workspace = Self::workspace_dir(package_root, library);
        std::iter::once(package_root.to_path_buf())
            .chain(workspace)
            .map(|dir| dir.join(MANIFEST_FILENAME))
            .filter(|path| path.is_file())
            .collect()
    }

    fn workspace_dir(package_root: &Path, library: bool) -> Option<PathBuf> {
        if library {
            return None;
        }
        crate::find_workspace_root(package_root).filter(|root| root != package_root)
    }

    fn read(dir: &Path) -> Result<Option<Self>, ManifestError> {
        let path = dir.join(MANIFEST_FILENAME);
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Ok(None);
        };
        let manifest: Self = toml::from_str(&content).map_err(|e| ManifestError::Invalid {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let problems: Vec<String> = manifest
            .telar
            .fonts
            .iter()
            .flat_map(FontDeclaration::problems)
            .chain(crate::prelude::problems(manifest.telar.prelude()))
            .chain(manifest.telar.library_theme_problem())
            .collect();
        if !problems.is_empty() {
            return Err(ManifestError::Invalid {
                path,
                message: problems.join("; "),
            });
        }
        Ok(Some(manifest))
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
            prelude: self.prelude.or(base.prelude),
            library: self.library,
            dev: DevSection {
                window: self.dev.window.or(base.dev.window),
                devtools: self.dev.devtools.or(base.dev.devtools),
            },
            i18n: I18nSection {
                root: self.i18n.root.or(base.i18n.root),
                scan: self.i18n.scan.or(base.i18n.scan),
                default: self.i18n.default.or(base.i18n.default),
            },
            web: WebSection {
                template: self.web.template.or(base.web.template),
                public: self.web.public.or(base.web.public),
                origin: self.web.origin.or(base.web.origin),
                base: self.web.base.or(base.web.base),
                description: self.web.description.or(base.web.description),
                og_image: self.web.og_image.or(base.web.og_image),
                theme_color: self.web.theme_color.or(base.web.theme_color),
                host: self.web.host.or(base.web.host),
                prerender: self.web.prerender.over(base.web.prerender),
            },
            // Whole rather than merged: a package naming any face of its own is declaring the set it ships.
            fonts: if self.fonts.is_empty() {
                base.fonts
            } else {
                self.fonts
            },
            locales: self.locales.or(base.locales),
            default_locale: self.default_locale.or(base.default_locale),
        }
    }

    /// Why a library naming a theme type is refused. It ships one artifact for every application that will depend on it, and none of their theme types is known when it is transpiled, so its `$theme` reads the `ThemeTokens` vocabulary every theme answers instead.
    fn library_theme_problem(&self) -> Option<String> {
        let theme = self.theme.as_deref().filter(|_| self.library)?;
        Some(format!(
            "`[telar] theme = \"{theme}\"` cannot be set in a `[telar] library`: a library is compiled into applications whose theme types it cannot name, so its `$theme.x` reads the shared `ThemeTokens` tokens instead. Remove the `theme` key"
        ))
    }

    /// The crates this package's `.rsx` glob-imports, in the order declared.
    pub fn prelude(&self) -> &[PreludeEntry] {
        self.prelude.as_deref().unwrap_or_default()
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
