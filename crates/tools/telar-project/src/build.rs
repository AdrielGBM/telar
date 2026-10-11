//! Which shape a package's `.rsx` is compiled into, where that shape lands, and the index describing what a transpile left behind.

mod artifact;

use std::path::{Path, PathBuf};

pub use artifact::{
    BUILD_ARTIFACT_FORMAT, BuildEntry, BuildFailure, BuildIndex, clear_build_failure,
    read_build_failure, read_build_index, relative_source, write_build_failure, write_build_index,
};

/// Which shape a package's `.rsx` is compiled into: hot-reloadable or not.
///
/// Each is written to its own directory on purpose: they are different Rust for the same source, and sharing one directory had each flavour — and the editor's mirror, which always writes the plain one — overwrite the other on every build, leaving every cargo unit permanently stale.
///
/// `[preview]` blocks are not on this axis. Every flavour emits them inside `::telar::__previews!`, which compiles them to nothing unless `telar/previews` is on, so a library compiled as a dependency — always Plain — carries its previews into the application that lists it, and a shipping build still contains none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BuildFlavour {
    /// What `cargo build`, `cargo telar build`, `check` and `test` produce, and what the editor's mirror writes.
    #[default]
    Plain,
    /// What `cargo telar dev` and `cargo telar preview` produce: signal declarations become keyed `hot_signal_auto!` bindings, so the dev host can carry their values across a dylib swap.
    Hot,
}

impl BuildFlavour {
    /// Every directory name a flavour writes under, for a reader that has only a path and needs to know whether it is generated output — `cargo-telar`'s diagnostic projection, which sees whichever one the build used.
    pub const DIR_NAMES: [&'static str; 2] = ["build", "build-hot"];

    /// Every flavour, for a producer that writes them all rather than guessing which a build will read.
    pub const ALL: [Self; 2] = [Self::Plain, Self::Hot];

    /// The flavour for a build that is `hot` or not.
    pub fn new(hot: bool) -> Self {
        match hot {
            false => Self::Plain,
            true => Self::Hot,
        }
    }

    /// The `.telar/` subdirectory this flavour writes into.
    pub fn dir_name(self) -> &'static str {
        match self {
            Self::Plain => "build",
            Self::Hot => "build-hot",
        }
    }

    /// The file this flavour writes in each placement site's `.telar/`, named per flavour so a `cargo telar dev` and a plain build do not overwrite each other's module tree.
    pub fn site_file_name(self) -> &'static str {
        match self {
            Self::Plain => "modules.rs",
            Self::Hot => "modules-hot.rs",
        }
    }

    /// The index this flavour writes beside its directory, naming the flavour so no two read each other's.
    pub fn index_filename(self) -> &'static str {
        match self {
            Self::Plain => "build.json",
            Self::Hot => "build-hot.json",
        }
    }

    /// Where a transpile that failed leaves its reason, beside the index it could not write.
    pub fn failure_filename(self) -> &'static str {
        match self {
            Self::Plain => "build.failure.json",
            Self::Hot => "build-hot.failure.json",
        }
    }

    pub fn is_hot(self) -> bool {
        matches!(self, Self::Hot)
    }
}

/// Where a package's generated Rust for `flavour` lives.
pub fn generated_dir(package_dir: &Path, flavour: BuildFlavour) -> PathBuf {
    package_dir.join(".telar").join(flavour.dir_name())
}

#[cfg(test)]
#[path = "build_test.rs"]
mod tests;
