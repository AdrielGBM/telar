//! Resolving a `.rsx`'s `src:"path"` against the artifact `cargo telar bake` wrote, and — when it cannot be resolved — saying exactly which command produces it.
//!
//! There is one path and no fallback: the transpiler never opens an asset to decode it, so a `src:"…"` is either a reference into the generated module or a `compile_error!`. That is what keeps `usvg`, `resvg` and `image` out of every project's proc-macro graph, and it is why every message here names a command instead of a feature — a build that reached one of them is missing a step, not a knob.
//!
//! Every message lives in this file rather than at its call site, so the wording of "what do I run" is decided once. The mismatch they describe is always between the artifact and the `telar` **the project resolves**, never the installed `cargo-telar`: a project may pin an older `telar` than the CLI that baked it, which is the case [`AssetIndex::telar_version`] exists to catch.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::artifact::{
    ASSETS_INDEX_FILENAME, ASSETS_MODULE, ASSETS_SOURCE_FILENAME, ArtifactHandshake, AssetEntry,
    AssetIndex, baked_init_expr, check_artifact, content_hash,
};
use super::{AssetKind, IdBaking, asset_kind_for_component};

/// What a package's `.telar/` had to say. `Unusable` carries the mismatch already phrased, so a caller that hits it for ten different assets writes the same explanation ten times without re-deriving it.
enum ArtifactState {
    NotBaked,
    Unusable(String),
    Ready(AssetIndex),
}

/// One package's baked asset artifact, loaded once and consulted per `src:"…"` and per baked id.
pub struct AssetContext {
    root: PathBuf,
    telar_dir: PathBuf,
    telar_version: String,
    state: ArtifactState,
    /// The package's `[telar]` table, which says how it bakes and spells the ids a component-named kind's prop is given.
    telar: crate::TelarSection,
    module_source: OnceLock<Option<String>>,
}

/// An id baked into the artifact, as a macro naming it from Rust inlines it at its call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BakedId {
    /// The id spelled the way the bake keyed it, so a bare icon name arrives read in its default set.
    pub id: String,
    /// See [`BakedAsset::monochrome`](super::BakedAsset::monochrome).
    pub monochrome: bool,
    /// The runtime type [`Self::init_expr`] builds, [`AssetKind::data_ty`].
    pub data_ty: &'static str,
    /// The expression the artifact initializes the asset's `static` with. It names `telar`'s items unqualified, as the generated module glob-imports them.
    pub init_expr: String,
}

impl AssetContext {
    /// Reads `<package_dir>/.telar/assets.json` and runs the version handshake against `current_telar_version` — the version of `telar` whose APIs the generated module calls, which is the caller's own `env!("CARGO_PKG_VERSION")` for `telar-macros` and the project's resolved `telar` elsewhere. Never fails: a missing, stale or unreadable artifact is a state every `src:"…"` reports for itself, on its own `.rsx` line, rather than a whole-project error on a project that may reference no asset at all.
    pub fn load(package_dir: &Path, current_telar_version: &str) -> Self {
        let telar_dir = package_dir.join(".telar");
        let state = match check_artifact(&telar_dir, current_telar_version) {
            Ok(ArtifactHandshake::NotBaked) => ArtifactState::NotBaked,
            Ok(ArtifactHandshake::UpToDate(index)) => ArtifactState::Ready(index),
            Ok(ArtifactHandshake::FormatMismatch { found, expected }) => ArtifactState::Unusable(
                format!("it is in format {found}, and this build of telar reads format {expected}"),
            ),
            Ok(ArtifactHandshake::VersionMismatch { found, expected }) => ArtifactState::Unusable(
                format!("it was baked for telar {found}, and this project builds telar {expected}"),
            ),
            Err(e) => ArtifactState::Unusable(format!("it could not be read: {e}")),
        };
        Self {
            root: crate::assets_root(package_dir),
            telar_dir,
            telar_version: current_telar_version.to_string(),
            state,
            telar: crate::TelarManifest::load_or_default(package_dir).telar,
            module_source: OnceLock::new(),
        }
    }

    /// The loaded index, or `None` when the artifact is missing or unusable — in which case every `src:"…"` in this package resolves to a `compile_error!` naming the command that fixes it.
    pub fn index(&self) -> Option<&AssetIndex> {
        match &self.state {
            ArtifactState::Ready(index) => Some(index),
            _ => None,
        }
    }

    /// The generated module to wire in as `#[path] pub mod __rsx_assets;`, or `None` when there is nothing usable to wire. Declaring it against an unusable artifact would fail inside generated code instead of on the `.rsx` line that asked for the asset.
    pub fn module_file(&self) -> Option<PathBuf> {
        self.index()?;
        let path = self.telar_dir.join(ASSETS_SOURCE_FILENAME);
        path.is_file().then_some(path)
    }

    /// The index file itself, so a caller that read it can make its build depend on it. Re-baking changes this file, and a macro that doesn't track it keeps answering from the copy it read on the last build.
    pub fn index_file(&self) -> Option<PathBuf> {
        let path = self.telar_dir.join(ASSETS_INDEX_FILENAME);
        path.is_file().then_some(path)
    }

    /// Every source asset the artifact was baked from, for a caller to register as a build input. Filtered to files that still exist: one deleted since the bake is already reported by [`Self::resolve`] against the `.rsx` that names it, which is a better place for it than a missing-file error on generated code.
    pub fn tracked_files(&self) -> Vec<PathBuf> {
        self.index()
            .map(|index| {
                index
                    .entries
                    .iter()
                    .filter(|entry| {
                        super::asset_kind_for_id(&entry.kind)
                            .is_some_and(|kind| kind.component.is_none())
                    })
                    .map(|entry| self.root.join(&entry.path))
                    .filter(|path| path.is_file())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The path expression a widget's `src:"rel"` becomes — `crate::__rsx_assets::ASSET_…` — or the message explaining why it cannot be one.
    pub fn resolve(&self, kind: &AssetKind, rel: &str) -> Result<String, String> {
        let index = match &self.state {
            ArtifactState::NotBaked => return Err(self.not_baked_message(kind, rel)),
            ArtifactState::Unusable(why) => {
                return Err(format!("rsx: {}", self.unusable_message(kind, rel, why)));
            }
            ArtifactState::Ready(index) => index,
        };
        let path = rel.replace('\\', "/");
        let Some(entry) = index
            .entries
            .iter()
            .find(|entry| entry.kind == kind.id && entry.path == path)
        else {
            return Err(format!(
                "rsx: the baked artifact does not list the {} asset `{rel}`. Run: cargo telar bake (or build through cargo telar check/dev/build, which bake first).",
                kind.label
            ));
        };

        let file = self.root.join(&path);
        let bytes = std::fs::read(&file).map_err(|e| {
            format!(
                "rsx: the {} asset `{rel}` is in the baked artifact but no longer at {}: {e}. Run: cargo telar bake",
                kind.label,
                file.display()
            )
        })?;
        if content_hash(&bytes) != entry.hash {
            return Err(format!(
                "rsx: the {} asset `{rel}` has changed since it was baked, so the artifact holds its old contents. Run: cargo telar bake (or build through cargo telar check/dev/build, which bake first).",
                kind.label
            ));
        }

        Ok(format!("crate::{ASSETS_MODULE}::{}", entry.static_name))
    }

    /// The component-named kind whose id `prop` carries on `tag`, and how this package bakes it. `None` when the package bakes nothing there, and the prop then receives the id as written.
    pub fn id_baking(&self, tag: &str, prop: &str) -> Option<(&'static AssetKind, IdBaking)> {
        let kind = asset_kind_for_component(tag, prop)?;
        let baking = match self.telar.id_baking(kind) {
            IdBaking::Required if kind.component.is_some_and(|c| c.carries(tag, prop)) => {
                IdBaking::Literals
            }
            baking => baking,
        };
        (baking != IdBaking::Off).then_some((kind, baking))
    }

    /// The value a literal id given to `prop` on `tag` becomes, or the message explaining why it cannot be one. Where the package bakes that kind, it is the triple [`Self::resolve_id`] answers; where the package configures the kind without baking it, the id spelled out in full, so a bare name reaches the runtime already read in its default set. `None` where the package does not configure the kind, and the literal reaches the component as written.
    pub fn literal_id(
        &self,
        tag: &str,
        prop: &str,
        written: &str,
    ) -> Option<Result<String, String>> {
        let kind = asset_kind_for_component(tag, prop)?;
        Some(match self.telar.id_baking(kind) {
            IdBaking::Off => self
                .telar
                .canonical_id(kind, written)?
                .map(|id| format!("{id:?}"))
                .map_err(|message| format!("rsx: {message}")),
            IdBaking::Literals | IdBaking::Required => self.resolve_prop_id(kind, prop, written),
        })
    }

    /// The value a baked id becomes, `("set:name", Arc::clone(&crate::__rsx_assets::ASSET_…), monochrome)` — the triple a component-named kind's prop accepts — or the message explaining why it cannot be one. `written` is read the way the bake keyed it, so a bare icon name finds the icon of the default set.
    ///
    /// Unlike a path, there is no file here to re-hash: the id is either in the artifact or it is not, and whether its source changed is the bake's question, which runs before every build route.
    pub fn resolve_id(&self, kind: &AssetKind, written: &str) -> Result<String, String> {
        self.resolve_prop_id(kind, kind.attr, written)
    }

    /// [`Self::resolve_id`] for an id given to `prop`, which a message names it by.
    fn resolve_prop_id(
        &self,
        kind: &AssetKind,
        prop: &str,
        written: &str,
    ) -> Result<String, String> {
        let (id, entry) = self
            .baked_entry(kind, written, |id| match written == id {
                true => format!("{prop}:\"{id}\""),
                false => format!("{prop}:\"{written}\" (`{id}`)"),
            })
            .map_err(|message| format!("rsx: {message}"))?;
        Ok(format!(
            "({id:?}, ::std::sync::Arc::clone(&crate::{ASSETS_MODULE}::{}), {})",
            entry.static_name, entry.monochrome
        ))
    }

    /// The baked id a Rust macro names as `tag!("written")`, where `tag` is the component that draws `kind`, such as `icon!("mdi:home")`: what the macro inlines at its call site, or the message explaining why it cannot. Unlike [`Self::resolve_id`] it needs no module wired by `rsx_modules!`, so a crate with no `.rsx` at all, such as a widget library written in Rust, can name what its bake baked.
    pub fn baked_id(&self, kind: &AssetKind, written: &str) -> Result<BakedId, String> {
        let Some(component) = kind.component else {
            return Err(format!(
                "the {} kind is baked from a path, not named by id",
                kind.label
            ));
        };
        let call = format!("{}!(\"{written}\")", component.tag);
        if self.telar.id_baking(kind) == IdBaking::Off {
            return Err(format!(
                "`{call}` names a {label} baked into this package, and its telar.toml bakes none: name where they come from in `[telar.{section}]`, leaving `mode` unset or setting it to \"both\"",
                label = kind.label,
                section = component.section
            ));
        }
        let (id, entry) = self.baked_entry(kind, written, |id| match written == id {
            true => format!("`{call}`"),
            false => format!("`{call}` (`{id}`)"),
        })?;
        let init_expr = self
            .module_source()
            .and_then(|source| baked_init_expr(source, &entry.static_name))
            .ok_or_else(|| {
                format!(
                    "the baked artifact lists the {} `{id}`, and `.telar/{ASSETS_SOURCE_FILENAME}` holds no initializer for it. Run: cargo telar bake",
                    kind.label
                )
            })?;
        Ok(BakedId {
            id,
            monochrome: entry.monochrome,
            data_ty: kind.data_ty,
            init_expr,
        })
    }

    /// The id `written` names, spelled the way the bake keyed it, and its entry in the artifact. `shown` spells the id for a message the way its caller wrote it.
    fn baked_entry(
        &self,
        kind: &AssetKind,
        written: &str,
        shown: impl Fn(&str) -> String,
    ) -> Result<(String, &AssetEntry), String> {
        let id = match self.telar.canonical_id(kind, written) {
            Some(Ok(id)) => id,
            Some(Err(message)) => return Err(message),
            None => written.to_string(),
        };
        let shown = shown(&id);
        let index = match &self.state {
            ArtifactState::NotBaked => {
                return Err(format!(
                    "no baked artifact for the {} {shown}. {}",
                    kind.label,
                    self.bake_hint(kind)
                ));
            }
            ArtifactState::Unusable(why) => return Err(self.unusable_message(kind, &id, why)),
            ArtifactState::Ready(index) => index,
        };
        let Some(entry) = index
            .entries
            .iter()
            .find(|entry| entry.kind == kind.id && entry.path == id)
        else {
            return Err(format!(
                "the baked artifact holds no {} {shown}. The bake reports an id it could not resolve, so check its output, or the id, against the sources `[telar.{}]` names. {}",
                kind.label,
                kind.component.map_or("", |component| component.section),
                self.bake_hint(kind)
            ));
        };
        Ok((id, entry))
    }

    fn module_source(&self) -> Option<&str> {
        self.module_source
            .get_or_init(|| {
                self.module_file()
                    .and_then(|path| std::fs::read_to_string(path).ok())
            })
            .as_deref()
    }

    /// Why `prop` on `tag`, which has to be baked, was given something that is not a literal id.
    pub fn dynamic_id_message(kind: &AssetKind, tag: &str, prop: &str) -> String {
        let section = kind.component.map_or("", |component| component.section);
        let drawn_by = kind.component.map_or("", |component| component.tag);
        format!(
            "rsx: `{tag} {prop}:` takes a literal id while `[telar.{section}]` bakes every {}, because only a literal can be baked ahead of time. Write the id out, choosing between literal ids with `if` or `match` where it varies; or set `[telar.{section}] mode = \"both\"` to resolve ids that are not literal as the application runs, or `mode = \"runtime\"` to resolve all of them then — see the `runtime` feature of the crate that provides `{drawn_by}`.",
            kind.label
        )
    }

    fn bake_hint(&self, kind: &AssetKind) -> String {
        format!(
            "A {} id is baked by the CLI, not by the compiler. Run: cargo telar bake (or build through cargo telar check/dev/build, which bake first). Install it with: cargo install cargo-telar --version {}",
            kind.label, self.telar_version
        )
    }

    /// A `src:"…"` transpiled with no package to anchor it — an in-memory transpile with no `.telar/` to look in, such as a unit test or an analyzer query on a detached buffer.
    pub fn detached_message(kind: &AssetKind, rel: &str) -> String {
        format!(
            "rsx: cannot resolve the {} asset `{rel}`: this transpile has no package directory, so there is no baked artifact to read.",
            kind.label
        )
    }

    fn not_baked_message(&self, kind: &AssetKind, rel: &str) -> String {
        format!(
            "rsx: no baked artifact for the {} asset `{rel}`. Static src:\"…\" assets are baked by the CLI, not by the compiler. Run: cargo telar bake (or build through cargo telar check/dev/build, which bake first). Install it with: cargo install cargo-telar --version {}",
            kind.label, self.telar_version
        )
    }

    fn unusable_message(&self, kind: &AssetKind, rel: &str, why: &str) -> String {
        format!(
            "the baked artifact cannot be used for the {} asset `{rel}`: {why}. Run: cargo install cargo-telar --version {} && cargo telar bake",
            kind.label, self.telar_version
        )
    }
}

#[cfg(test)]
#[path = "context_test.rs"]
mod tests;
