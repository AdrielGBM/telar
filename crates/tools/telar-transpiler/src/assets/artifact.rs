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
//! What is deliberately *not* here: deciding whether a `format`/`telar_version` mismatch is fatal (that
//! policy is a later task) and turning file bytes into a [`BakedAsset`] in the first place (the baker's
//! job) — this module only defines the shape both sides agree on, and reads/writes it.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Bumped whenever `.telar/assets.rs`'s generated source changes shape in a way older `telar`/`telar-macros`
/// can't read (e.g. the `static`'s wrapper type, or how an entry's initializer is called). Compared against
/// an index's own [`AssetIndex::format`] by whoever enforces the handshake.
pub const ASSET_ARTIFACT_FORMAT: u32 = 1;

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
    let mut used_types: Vec<&'static str> = Vec::new();
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
        if !used_types.contains(&kind.data_ty) {
            used_types.push(kind.data_ty);
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

    used_types.sort_unstable();
    let mut source = String::from("#[allow(unused_imports)] use std::sync::{Arc, LazyLock};\n");
    if !used_types.is_empty() {
        source.push_str(&format!("use telar::{{{}}};\n", used_types.join(", ")));
    }
    source.push('\n');
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

/// Writes `<telar_dir>/assets.json` and `<telar_dir>/assets.rs`, atomically and only when their content
/// changed — same convention as the analyzer's `write_if_changed` (`telar-analyzer/src/build_sync.rs`), so
/// a bake that changes nothing doesn't retrigger rustc or rust-analyzer's file watcher.
pub fn write_generated(telar_dir: &Path, generated: &GeneratedAssets) -> std::io::Result<()> {
    std::fs::create_dir_all(telar_dir)?;
    write_if_changed_atomic(
        &telar_dir.join(ASSETS_INDEX_FILENAME),
        &format!("{}\n", generated.index.to_json()),
    )?;
    write_if_changed_atomic(&telar_dir.join(ASSETS_SOURCE_FILENAME), &generated.source)?;
    Ok(())
}

/// Writes via a same-directory temp file plus rename, so a reader (rustc, rust-analyzer) never observes a
/// half-written file — the analyzer and the CLI can legitimately race to bake the same package.
fn write_if_changed_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path)
        .map(|existing| existing == content)
        .unwrap_or(false)
    {
        return Ok(());
    }
    let tmp_name = format!(
        "{}.tmp-{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("assets"),
        std::process::id()
    );
    let tmp_path = path.with_file_name(tmp_name);
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svg_asset(path: &str, content: &[u8]) -> BakedAsset {
        BakedAsset {
            kind: "svg".to_string(),
            path: path.to_string(),
            content: content.to_vec(),
            init_expr: "SvgData::from_baked_vector(&[])".to_string(),
        }
    }

    fn image_asset(path: &str, content: &[u8]) -> BakedAsset {
        BakedAsset {
            kind: "image".to_string(),
            path: path.to_string(),
            content: content.to_vec(),
            init_expr: "ImageData::new(vec![0; 4], 1, 1)".to_string(),
        }
    }

    #[test]
    fn index_round_trips_through_json() {
        let index = AssetIndex {
            format: ASSET_ARTIFACT_FORMAT,
            producer: "cargo-telar 0.1.8".to_string(),
            telar_version: "0.1.8".to_string(),
            entries: vec![AssetEntry {
                kind: "svg".to_string(),
                path: "badge.svg".to_string(),
                hash: content_hash(b"<svg/>"),
                static_name: static_name_for_path("badge.svg"),
            }],
        };
        let parsed = AssetIndex::from_json(&index.to_json()).unwrap();
        assert_eq!(parsed, index);
    }

    #[test]
    fn from_json_reports_a_malformed_index() {
        let err = AssetIndex::from_json("{ not json").unwrap_err();
        assert!(err.contains("malformed asset index"), "{err}");
    }

    #[test]
    fn content_hash_reflects_bytes_not_path() {
        assert_eq!(content_hash(b"hello"), content_hash(b"hello"));
        assert_ne!(content_hash(b"hello"), content_hash(b"world"));
    }

    #[test]
    fn static_name_is_stable_across_slash_styles() {
        assert_eq!(
            static_name_for_path("icons/badge.svg"),
            static_name_for_path("icons\\badge.svg")
        );
    }

    #[test]
    fn generates_one_static_per_asset_with_the_right_wrapper_type() {
        let asset = svg_asset("icons/badge.svg", b"<svg/>");
        let generated =
            generate_assets(std::slice::from_ref(&asset), "cargo-telar 0.1.8", "0.1.8").unwrap();
        let name = static_name_for_path("icons/badge.svg");

        assert!(
            generated
                .source
                .contains(&format!("pub static {name}: LazyLock<Arc<SvgData>>")),
            "{}",
            generated.source
        );
        assert!(
            generated.source.contains("use telar::{SvgData};"),
            "{}",
            generated.source
        );
        assert_eq!(generated.index.entries.len(), 1);
        assert_eq!(generated.index.entries[0].static_name, name);
        assert_eq!(generated.index.entries[0].hash, content_hash(b"<svg/>"));
        assert_eq!(generated.index.format, ASSET_ARTIFACT_FORMAT);
    }

    #[test]
    fn only_the_kinds_actually_used_are_imported() {
        let generated = generate_assets(&[svg_asset("a.svg", b"1")], "test", "0.1.8").unwrap();
        assert!(
            !generated.source.contains("ImageData"),
            "{}",
            generated.source
        );
    }

    #[test]
    fn static_names_are_independent_of_entry_order() {
        let badge = svg_asset("badge.svg", b"<svg/>");
        let logo = image_asset("logo.png", b"PNGDATA");

        let forward = generate_assets(&[badge.clone(), logo.clone()], "t", "0.1.8").unwrap();
        let backward = generate_assets(&[logo, badge], "t", "0.1.8").unwrap();

        assert_eq!(forward.source, backward.source);
        assert_eq!(forward.index.entries, backward.index.entries);
    }

    #[test]
    fn adding_an_asset_does_not_rename_the_others() {
        let badge = svg_asset("badge.svg", b"<svg/>");
        let logo = image_asset("logo.png", b"PNGDATA");
        let before = generate_assets(&[badge.clone(), logo.clone()], "t", "0.1.8").unwrap();
        let badge_name = |g: &GeneratedAssets| {
            g.index
                .entries
                .iter()
                .find(|e| e.path == "badge.svg")
                .unwrap()
                .static_name
                .clone()
        };
        let logo_name = |g: &GeneratedAssets| {
            g.index
                .entries
                .iter()
                .find(|e| e.path == "logo.png")
                .unwrap()
                .static_name
                .clone()
        };

        let icon = svg_asset("icon.svg", b"<svg id=\"x\"/>");
        let after = generate_assets(&[badge, logo, icon], "t", "0.1.8").unwrap();

        assert_eq!(badge_name(&before), badge_name(&after));
        assert_eq!(logo_name(&before), logo_name(&after));
        assert_eq!(after.index.entries.len(), 3);
    }

    #[test]
    fn duplicate_paths_are_rejected() {
        let err = generate_assets(
            &[svg_asset("badge.svg", b"1"), svg_asset("badge.svg", b"2")],
            "t",
            "0.1.8",
        )
        .unwrap_err();
        assert!(err.contains("badge.svg"), "{err}");
    }

    #[test]
    fn unknown_kinds_are_rejected() {
        let asset = BakedAsset {
            kind: "font".to_string(),
            path: "sans.ttf".to_string(),
            content: b"1".to_vec(),
            init_expr: "x".to_string(),
        };
        let err = generate_assets(&[asset], "t", "0.1.8").unwrap_err();
        assert!(err.contains("font"), "{err}");
    }

    #[test]
    fn read_index_of_a_never_baked_package_is_none() {
        let dir = std::env::temp_dir().join(format!(
            "rsx_assets_artifact_missing_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        assert!(read_index(&dir).unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_then_read_round_trips() {
        let dir =
            std::env::temp_dir().join(format!("rsx_assets_artifact_rw_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let telar_dir = dir.join(".telar");

        let generated = generate_assets(
            &[svg_asset("badge.svg", b"<svg/>")],
            "cargo-telar 0.1.8",
            "0.1.8",
        )
        .unwrap();
        write_generated(&telar_dir, &generated).unwrap();

        let read_back = read_index(&telar_dir).unwrap().unwrap();
        assert_eq!(read_back, generated.index);
        let source_on_disk =
            std::fs::read_to_string(telar_dir.join(ASSETS_SOURCE_FILENAME)).unwrap();
        assert_eq!(source_on_disk, generated.source);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn write_generated_skips_disk_io_when_content_is_unchanged() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!(
            "rsx_assets_artifact_unchanged_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let telar_dir = dir.join(".telar");

        let generated = generate_assets(
            &[svg_asset("badge.svg", b"<svg/>")],
            "cargo-telar 0.1.8",
            "0.1.8",
        )
        .unwrap();
        write_generated(&telar_dir, &generated).unwrap();

        // A second write with identical content would fail with "permission denied" instead of silently succeeding, proving the skip without racing on mtime.
        for name in [ASSETS_INDEX_FILENAME, ASSETS_SOURCE_FILENAME] {
            let path = telar_dir.join(name);
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o444);
            std::fs::set_permissions(&path, perms).unwrap();
        }

        write_generated(&telar_dir, &generated).expect("unchanged content must not be rewritten");

        for name in [ASSETS_INDEX_FILENAME, ASSETS_SOURCE_FILENAME] {
            let path = telar_dir.join(name);
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o644);
            std::fs::set_permissions(&path, perms).unwrap();
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
