//! Conversions between filesystem paths and [`lsp_types::Uri`].
//!
//! lsp-types 0.97 dropped `url::Url` (and its `from_file_path` / `to_file_path` helpers) for a minimal `fluent_uri`-based [`Uri`], so we bridge through the `url` crate, which already handles `file://` percent-encoding and platform path quirks (spaces, Unicode, Windows drive letters).

use std::path::{Path, PathBuf};
use std::str::FromStr;

use lsp_types::Uri;

/// Builds a `file://` [`Uri`] from a filesystem path.
pub fn from_path(path: &Path) -> Option<Uri> {
    let url = url::Url::from_file_path(path).ok()?;
    Uri::from_str(url.as_str()).ok()
}

/// Recovers the filesystem path from a `file://` [`Uri`].
pub fn to_path(uri: &Uri) -> Option<PathBuf> {
    url::Url::parse(uri.as_str()).ok()?.to_file_path().ok()
}
