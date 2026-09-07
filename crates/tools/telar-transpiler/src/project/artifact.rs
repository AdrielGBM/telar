//! The pre-transpiled build artifact: what `cargo telar transpile` leaves behind, and how the macro decides whether it still answers for the `.rsx` on disk.
//!
//! Same split as the baked asset artifact next door — an index the macro reads, beside generated Rust nobody parses. It is a *second* copy of a question the macro can also answer itself, so the only thing that matters here is being sure: an index that claims more than it can back makes the build compile last week's markup and say nothing. Hence a hash per source rather than a timestamp, and a set comparison rather than a count.
//!
//! The index sits beside its directory ([`super::BuildFlavour::index_filename`]) rather than inside it, because [`crate::prune_stale_generated`] sweeps that directory and the artifact has to survive a sweep that finds no live output at all.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::BuildFlavour;

/// Bumped whenever the generated Rust changes shape in a way an older `telar-macros` cannot wire — a different output path convention, a preview const that is named differently. Any mismatch sends the reader back to transpiling for itself, which is always correct and only slower.
pub const BUILD_ARTIFACT_FORMAT: u32 = 1;

/// One `.rsx` the artifact was written from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildEntry {
    /// Path to the `.rsx`, relative to the package's `src/`, with `/` separators so an artifact written on one platform reads on another.
    pub source: String,
    /// Content hash of that `.rsx` at transpile time — how a source edited since is detected.
    pub hash: String,
    /// Content hash of the generated Rust this produced. The editor mirrors an *unsaved* buffer into the same directory, so a source hash alone would let the build compile what the editor is showing rather than what is on disk — which is the one thing an artifact must never do quietly.
    pub output_hash: String,
    /// Whether the file declared any `[preview]`, which is the only thing about its *content* the wiring needs to know.
    pub previews: bool,
}

/// The index `.telar/build.json` (or `build-hot.json`) holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildIndex {
    /// [`BUILD_ARTIFACT_FORMAT`] at the time this was written.
    pub format: u32,
    /// What produced this file, for diagnostics — e.g. `"cargo-telar 0.1.8"`.
    pub producer: String,
    /// The `telar` version the generated Rust was written against, compared the same way and for the same reason as the asset artifact's.
    pub telar_version: String,
    /// The theme type the sources were transpiled against. A build naming a different one gets different Rust for the same markup, so it cannot use this.
    pub theme: Option<String>,
    /// Whether any generated file reaches into the baked asset module. Wiring that module is the reader's decision, not this one's, and it declines when the asset artifact is unusable — so output that references it has to be re-produced instead, or the reference resolves to nothing and rustc reports it against generated code rather than against the `.rsx` line that named the asset.
    pub uses_assets: bool,
    pub entries: Vec<BuildEntry>,
}

impl BuildIndex {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("BuildIndex holds only strings, bools and a Vec")
    }

    /// Whether this artifact can be wired as-is for the `.rsx` currently under `src_dir`.
    ///
    /// Every source has to be listed and unchanged, every listed source has to still be there, and the Rust each one names has to still be on disk: a file added since the transpile has no generated Rust to wire, one deleted since would be wired to output that is about to be pruned, and output deleted by hand would be wired to nothing at all — which rustc reports against a file nobody wrote.
    ///
    /// Reading the sources to hash them is the cost of being sure, and it is the same read the macro does anyway to emit its rerun triggers.
    pub fn answers_for(
        &self,
        src_dir: &Path,
        generated_dir: &Path,
        theme: Option<&str>,
        current_telar_version: &str,
    ) -> bool {
        if self.format != BUILD_ARTIFACT_FORMAT
            || self.telar_version != current_telar_version
            || self.theme.as_deref() != theme
        {
            return false;
        }
        let listed: HashMap<&str, &BuildEntry> = self
            .entries
            .iter()
            .map(|entry| (entry.source.as_str(), entry))
            .collect();
        let sources = crate::find_rsx_files(src_dir);
        if sources.len() != listed.len() {
            return false;
        }
        sources.iter().all(|path| {
            let Some(relative) = relative_source(path, src_dir) else {
                return false;
            };
            let Some(entry) = listed.get(relative.as_str()) else {
                return false;
            };
            let output_matches = crate::relative_output_path(path, src_dir)
                .and_then(|rel| std::fs::read(generated_dir.join(rel)).ok())
                .is_some_and(|bytes| crate::content_hash(&bytes) == entry.output_hash);
            output_matches
                && std::fs::read(path)
                    .map(|bytes| crate::content_hash(&bytes) == entry.hash)
                    .unwrap_or(false)
        })
    }

    /// The entry for a `.rsx`, by its path under `src_dir`.
    pub fn entry_for(&self, rsx_path: &Path, src_dir: &Path) -> Option<&BuildEntry> {
        let relative = relative_source(rsx_path, src_dir)?;
        self.entries.iter().find(|entry| entry.source == relative)
    }
}

/// The `source` key for a `.rsx`: its path under `src_dir`, always `/`-separated.
pub fn relative_source(rsx_path: &Path, src_dir: &Path) -> Option<String> {
    let relative = rsx_path.strip_prefix(src_dir).ok()?;
    Some(
        relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// Reads `<package_dir>/.telar/<flavour>.json`, or `None` when nothing has transpiled this package yet.
///
/// A malformed index reads as absent rather than as an error: the reader's answer to both is to transpile for itself, and a corrupt artifact is not something the person compiling can act on.
pub fn read_build_index(package_dir: &Path, flavour: BuildFlavour) -> Option<BuildIndex> {
    let path = index_path(package_dir, flavour);
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Writes the index beside the flavour's generated directory.
pub fn write_build_index(
    package_dir: &Path,
    flavour: BuildFlavour,
    index: &BuildIndex,
) -> std::io::Result<()> {
    let path = index_path(package_dir, flavour);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::write_if_changed_atomic(&path, &format!("{}\n", index.to_json()))
}

fn index_path(package_dir: &Path, flavour: BuildFlavour) -> std::path::PathBuf {
    package_dir.join(".telar").join(flavour.index_filename())
}

#[cfg(all(test, feature = "transpile"))]
#[path = "artifact_test.rs"]
mod tests;
