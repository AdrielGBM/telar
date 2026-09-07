//! The pre-baked asset artifact: two files a package's `.telar/` directory holds once `cargo telar bake`
//! has run — `assets.json`, the index `t!`-style macro expansion reads to emit a reference, and
//! `assets.rs`, the generated module holding the actual baked data. Same split as `.telar/build/__i18n.rs`
//! (generated Rust) has no JSON sibling only because nothing outside the macro itself ever needs to ask
//! "is this catalog stale" without re-parsing it; an asset artifact is read by three different binaries
//! (the macro, `telar-analyzer`, `cargo-telar`), so the machine-checkable question — is this entry's hash
//! still current — gets its own file instead of being buried in generated Rust.
//!
//! Neither file lives under `.telar/build/` (or `.telar/build-hot/`): that directory is swept clean by
//! [`crate::prune_stale_generated`] on every macro expansion, and the artifact is meant to survive a build
//! that produces no `.rsx` output at all (e.g. `cargo check` on an unrelated file). It is also independent
//! of the `build`/`build-hot` flavour split, since it decodes no reactive-vs-hot-reload distinction of its
//! own — both flavours reference the same `.telar/assets.rs`.
//!
//! [`check_artifact`] decides *whether* a `format`/`telar_version` mismatch exists and which of the two it
//! is; it does not decide what a caller should do about it or how to phrase that for a human — that is
//! [`crate::AssetContext`]'s job. Turning file bytes into a [`BakedAsset`] in the first place is the
//! baker's job, not this module's — this module only defines the shape both sides agree on, and reads,
//! writes, and validates it.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Bumped whenever `.telar/assets.rs`'s generated source changes shape in a way older `telar`/`telar-macros`
/// can't read (e.g. the `static`'s wrapper type, or how an entry's initializer is called). This is
/// mandatory, not discretionary: [`check_artifact`] treats any mismatch here as fatal on its own, without
/// even looking at `telar_version`, because a shape it can't parse makes that second comparison moot.
/// Compared against an index's own [`AssetIndex::format`] by [`check_artifact`].
pub const ASSET_ARTIFACT_FORMAT: u32 = 2;

/// The module every baked `static` is wired under once a macro declares `#[path = "…/assets.rs"] pub mod
/// __rsx_assets;`, mirroring [`crate::I18N_MODULE`]. Unused by this module itself — the generated source
/// has no `mod` wrapper of its own, exactly like `__i18n.rs` — but recorded here so the name is decided
/// once, before more than one task starts assuming a spelling for it.
pub const ASSETS_MODULE: &str = "__rsx_assets";

/// File name of the generated index, joined onto a package's `.telar/` directory.
pub const ASSETS_INDEX_FILENAME: &str = "assets.json";
/// File name of the generated Rust module, joined onto a package's `.telar/` directory.
pub const ASSETS_SOURCE_FILENAME: &str = "assets.rs";

/// One resource an artifact bakes: a `.rsx` referenced it through `kind.attr` (`src:"path"`), and the
/// baker turned its bytes into a `static` the generated module exports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetEntry {
    /// [`crate::AssetKind::id`] this resource bakes as.
    pub kind: String,
    /// Path to the source file, relative to the project's asset root ([`crate::assets_root`]).
    pub path: String,
    /// Content hash of the source file, hex-encoded — how a stale artifact is detected. Editing the file
    /// changes this without changing `static_name`, which depends only on `path`.
    pub hash: String,
    /// The `static` this entry is reachable as in the generated module, e.g. `ASSET_A1B2C3D4`.
    pub static_name: String,
}

/// The index `.telar/assets.json` holds. Written by the baker, read by the macro to emit
/// `crate::__rsx_assets::ASSET_…` references instead of decoding anything itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetIndex {
    /// [`ASSET_ARTIFACT_FORMAT`] at the time this was written.
    pub format: u32,
    /// What produced this file, for diagnostics — e.g. `"cargo-telar 0.1.8"`. Not part of the handshake:
    /// that's `telar_version` below.
    pub producer: String,
    /// The `telar`/`telar-macros` version this artifact's generated source was written against
    /// (`env!("CARGO_PKG_VERSION")` at bake time, since `telar` and `telar-macros` share a workspace
    /// version). `format` covers the index's own shape; this covers the emitted Rust calling into
    /// `telar`'s own signatures (`SvgData::from_baked_vector`, …), which can change independently of it.
    pub telar_version: String,
    pub entries: Vec<AssetEntry>,
}

impl AssetIndex {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("AssetIndex holds only strings and a Vec of them")
    }

    /// `Err` describes a malformed index — missing field, wrong type, truncated file. Never a version
    /// mismatch: that comparison happens once the fields are in hand, against whatever policy applies.
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("malformed asset index: {e}"))
    }
}

/// One resource the baker has already turned into Rust source, ready to enter the artifact. Deliberately
/// content-agnostic: `content` is only ever hashed here, never parsed — decoding bytes into a value is the
/// baker's job, not this module's.
#[derive(Debug, Clone)]
pub struct BakedAsset {
    /// [`crate::AssetKind::id`] this resource bakes as.
    pub kind: String,
    /// Path to the source file, relative to the project's asset root, `/`-separated.
    pub path: String,
    /// The source file's bytes, hashed for [`AssetEntry::hash`] and otherwise unused.
    pub content: Vec<u8>,
    /// Rust source for the baked value's initializer, e.g. `SvgData::from_baked_vector(&[..])` — the
    /// baker's output. Wrapped here in `Arc::new(..)` behind a `LazyLock`.
    pub init_expr: String,
}

/// What [`generate_assets`] produces: the index and the Rust source it was derived from, kept together so
/// a caller can never write one without the other.
#[derive(Debug, Clone)]
pub struct GeneratedAssets {
    pub index: AssetIndex,
    pub source: String,
}

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a over `bytes`: fast, dependency-free, and its output is fixed by the algorithm rather than by a
/// standard-library implementation detail — `std`'s `DefaultHasher` documents that its algorithm may
/// change between Rust versions, which would silently rename every `static` in this artifact on a
/// toolchain bump.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// The `static` an asset at `path` is reachable as, e.g. `ASSET_A1B2C3D4`. Derived from the path alone,
/// not a counter, so baking one more asset never renames another and the same asset gets the same name on
/// every machine. Backslashes fold to `/` first, so a project baked on Windows and on Linux agree.
pub fn static_name_for_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    format!(
        "ASSET_{:08X}",
        (fnv1a64(normalized.as_bytes()) & 0xFFFF_FFFF) as u32
    )
}

/// Content hash of a source asset's bytes, hex-encoded — see [`AssetEntry::hash`].
pub fn content_hash(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a64(bytes))
}

/// Turns already-baked resources into the artifact's two halves: the Rust module and the index describing
/// it, from the same pass so the two can never drift apart. `Err` when an entry can't be placed — an
/// unknown `kind`, a `path` listed twice, or (astronomically unlikely, but silent corruption otherwise) two
/// different paths whose hash collides on the same `static_name`.
///
/// Changing the shape of the source this emits — the `static`'s wrapper type, or how `init_expr` gets
/// invoked below — requires bumping [`ASSET_ARTIFACT_FORMAT`] in the same change. Nothing here enforces
/// that at compile time: an unbumped constant still lets [`check_artifact`] wave the new shape through as
/// `UpToDate`, so a reader built against the old shape fails on `assets.rs` itself instead of getting the
/// clean [`ArtifactHandshake::FormatMismatch`] it should have seen.
pub fn generate_assets(
    assets: &[BakedAsset],
    producer: &str,
    telar_version: &str,
) -> Result<GeneratedAssets, String> {
    let mut ordered: Vec<&BakedAsset> = assets.iter().collect();
    ordered.sort_by(|a, b| a.path.cmp(&b.path));

    let mut entries = Vec::with_capacity(ordered.len());
    let mut seen_paths = std::collections::BTreeSet::new();
    let mut seen_static_names: std::collections::BTreeMap<String, &str> =
        std::collections::BTreeMap::new();
    let mut body = String::new();

    for asset in ordered {
        if !seen_paths.insert(asset.path.as_str()) {
            return Err(format!("duplicate asset path `{}`", asset.path));
        }
        let kind = super::asset_kind_for_id(&asset.kind)
            .ok_or_else(|| format!("unknown asset kind `{}` for `{}`", asset.kind, asset.path))?;
        let static_name = static_name_for_path(&asset.path);
        if let Some(other) = seen_static_names.insert(static_name.clone(), asset.path.as_str()) {
            return Err(format!(
                "asset paths `{other}` and `{}` both hash to `{static_name}` — rename one of them",
                asset.path
            ));
        }
        body.push_str(&format!(
            "pub static {static_name}: LazyLock<Arc<{}>> = LazyLock::new(|| Arc::new({}));\n",
            kind.data_ty, asset.init_expr
        ));
        entries.push(AssetEntry {
            kind: asset.kind.clone(),
            path: asset.path.clone(),
            hash: content_hash(&asset.content),
            static_name,
        });
    }

    // A glob, and the same one every transpiled component file opens with: an `init_expr` names whatever types its baker chose (`VectorCommand`, `PathData`, `Point`, …), which is not derivable from the asset kind.
    let mut source = String::from(
        "#![allow(clippy::all)]\n#[allow(unused_imports)] use telar::*;\n#[allow(unused_imports)] use std::sync::{Arc, LazyLock};\n\n",
    );
    source.push_str(&body);

    Ok(GeneratedAssets {
        index: AssetIndex {
            format: ASSET_ARTIFACT_FORMAT,
            producer: producer.to_string(),
            telar_version: telar_version.to_string(),
            entries,
        },
        source,
    })
}

/// Reads `<telar_dir>/assets.json`. `Ok(None)` when the file doesn't exist — a project that bakes nothing,
/// or one that hasn't run `cargo telar bake` yet. A malformed file is `Err`, not `Ok(None)`: a caller reacts
/// to "never baked" and "baked something unreadable" differently, so the two must not look the same.
pub fn read_index(telar_dir: &Path) -> std::io::Result<Option<AssetIndex>> {
    match std::fs::read_to_string(telar_dir.join(ASSETS_INDEX_FILENAME)) {
        Ok(json) => AssetIndex::from_json(&json)
            .map(Some)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// What checking a package's artifact against this binary's own expectations turns up. Returned by
/// [`check_artifact`], which folds a [`read_index`] call and the version handshake into one call so a
/// caller matches on a single value instead of separately unwrapping an `Option` and comparing fields.
///
/// Deliberately keeps the not-baked, stale, and up-to-date cases apart as distinct variants rather than
/// collapsing them into a bool or an `Option`: each demands different behavior from a caller (fall back
/// silently, fail with instructions to re-bake, or proceed), and [`read_index`]'s `Err` already carries the
/// fourth case — a corrupt file — so nothing here needs to re-represent it. Composing the actual diagnostic
/// text for each variant is deliberately left to whoever calls this (macro expansion, `telar-analyzer`,
/// `cargo-telar` each want different phrasing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactHandshake {
    /// No `.telar/assets.json` — the package hasn't run `cargo telar bake` yet, or bakes no assets at all.
    NotBaked,
    /// `index.format` doesn't match [`ASSET_ARTIFACT_FORMAT`]. Checked before `telar_version`, and reported
    /// instead of it, because a format this binary doesn't recognize makes no promise about what
    /// `telar_version` even means in that shape.
    FormatMismatch { found: u32, expected: u32 },
    /// `index.format` matches, but `index.telar_version` doesn't match the `current_telar_version` the
    /// caller passed in. The index's own shape is fine, but the generated `assets.rs` may call `telar` APIs
    /// (`SvgData::from_baked_vector`, `ImageData::new`, …) whose signatures moved since this was baked —
    /// see [`AssetIndex::telar_version`] for why this check exists independently of `format`.
    VersionMismatch { found: String, expected: String },
    /// Both checks passed. Holds the parsed index so a caller that reached this variant never has to call
    /// [`read_index`] again to get at the entries.
    UpToDate(AssetIndex),
}

/// Reads `<telar_dir>/assets.json` and checks it against [`ASSET_ARTIFACT_FORMAT`] and
/// `current_telar_version`. Pass the *caller's own* `env!("CARGO_PKG_VERSION")` — not this crate's — since
/// the comparison is only meaningful against the binary that will actually load the generated
/// `assets.rs` (see [`AssetIndex::telar_version`]); `telar-transpiler`, `telar`, and `telar-macros` share a
/// workspace version today, but this function makes no assumption that they always will.
///
/// `Err` only for a corrupt or otherwise unreadable file, propagated from [`read_index`]. The three states
/// [`ArtifactHandshake`] distinguishes on the happy path — not baked, stale, up to date — are all `Ok`, so a
/// caller doesn't need to unwrap a `Result` just to learn the artifact was simply never baked.
pub fn check_artifact(
    telar_dir: &Path,
    current_telar_version: &str,
) -> std::io::Result<ArtifactHandshake> {
    let Some(index) = read_index(telar_dir)? else {
        return Ok(ArtifactHandshake::NotBaked);
    };
    if index.format != ASSET_ARTIFACT_FORMAT {
        return Ok(ArtifactHandshake::FormatMismatch {
            found: index.format,
            expected: ASSET_ARTIFACT_FORMAT,
        });
    }
    if index.telar_version != current_telar_version {
        return Ok(ArtifactHandshake::VersionMismatch {
            found: index.telar_version,
            expected: current_telar_version.to_string(),
        });
    }
    Ok(ArtifactHandshake::UpToDate(index))
}

/// Writes `<telar_dir>/assets.json` and `<telar_dir>/assets.rs`, atomically and only when their content
/// changed — same convention as the analyzer's `write_if_changed` (`telar-analyzer/src/build_sync.rs`), so
/// a bake that changes nothing doesn't retrigger rustc or rust-analyzer's file watcher.
pub fn write_generated(telar_dir: &Path, generated: &GeneratedAssets) -> std::io::Result<()> {
    std::fs::create_dir_all(telar_dir)?;
    crate::write_if_changed_atomic(
        &telar_dir.join(ASSETS_INDEX_FILENAME),
        &format!("{}\n", generated.index.to_json()),
    )?;
    crate::write_if_changed_atomic(&telar_dir.join(ASSETS_SOURCE_FILENAME), &generated.source)?;
    Ok(())
}

#[cfg(test)]
#[path = "artifact_test.rs"]
mod tests;
