//! Which shape a package's `.rsx` is compiled into, where that shape lands, and the index describing what a transpile left behind.

mod artifact;

use std::path::{Path, PathBuf};

pub use artifact::{
    BUILD_ARTIFACT_FORMAT, BuildEntry, BuildIndex, read_build_index, relative_source,
    write_build_index,
};

/// Which shape a package's `.rsx` is compiled into: hot-reloadable or not, carrying `[preview]` fns or not.
///
/// Each is written to its own directory on purpose: they are different Rust for the same source, and sharing one directory had each flavour — and the editor's mirror, which always writes the plain one — overwrite the other on every build, leaving every cargo unit permanently stale. Previews are on this axis for exactly that reason: alternating `cargo telar dev` and `cargo telar preview` would otherwise rewrite every generated file each time, and rustc would rebuild the crate on every switch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BuildFlavour {
    /// What `cargo build` and `cargo telar build` produce, and what the editor's mirror writes.
    #[default]
    Plain,
    /// What `cargo telar dev` produces: signal declarations become keyed `hot_signal_auto!` bindings, so the dev host can carry their values across a dylib swap.
    Hot,
    /// The same as [`Self::Plain`], plus a build fn per `[preview]` and the table naming them. What `cargo telar check` and `cargo telar test` produce — the two commands that answer for a preview without running one in a window.
    Preview,
    /// Both: `cargo telar preview`, which reloads previews in a window.
    HotPreview,
}

impl BuildFlavour {
    /// Every directory name a flavour writes under, for a reader that has only a path and needs to know whether it is generated output — `cargo-telar`'s diagnostic projection, which sees whichever one the build used.
    pub const DIR_NAMES: [&'static str; 4] =
        ["build", "build-hot", "build-preview", "build-hot-preview"];

    /// Every flavour, for a producer that writes them all rather than guessing which a build will read.
    pub const ALL: [Self; 4] = [Self::Plain, Self::Hot, Self::Preview, Self::HotPreview];

    /// The flavour for a build that is `hot` and/or carries `previews`.
    pub fn new(hot: bool, previews: bool) -> Self {
        match (hot, previews) {
            (false, false) => Self::Plain,
            (true, false) => Self::Hot,
            (false, true) => Self::Preview,
            (true, true) => Self::HotPreview,
        }
    }

    /// The `.telar/` subdirectory this flavour writes into.
    pub fn dir_name(self) -> &'static str {
        match self {
            Self::Plain => "build",
            Self::Hot => "build-hot",
            Self::Preview => "build-preview",
            Self::HotPreview => "build-hot-preview",
        }
    }

    /// The index this flavour writes beside its directory, naming the flavour so no two read each other's.
    pub fn index_filename(self) -> &'static str {
        match self {
            Self::Plain => "build.json",
            Self::Hot => "build-hot.json",
            Self::Preview => "build-preview.json",
            Self::HotPreview => "build-hot-preview.json",
        }
    }

    pub fn is_hot(self) -> bool {
        matches!(self, Self::Hot | Self::HotPreview)
    }

    pub fn has_previews(self) -> bool {
        matches!(self, Self::Preview | Self::HotPreview)
    }
}

/// Where a package's generated Rust for `flavour` lives.
pub fn generated_dir(package_dir: &Path, flavour: BuildFlavour) -> PathBuf {
    package_dir.join(".telar").join(flavour.dir_name())
}

#[cfg(test)]
#[path = "build_test.rs"]
mod tests;
