//! Matrices: one preview rendered across several environments at once.

/// The cells a preview is rendered across.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Matrix {
    /// A matrix the package names under `[telar.previews.matrices]`.
    Named(&'static str),
}
