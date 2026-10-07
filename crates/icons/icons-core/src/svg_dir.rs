//! [`SvgDir`]: an application's own SVGs, addressed by Iconify ids.

use std::path::{Path, PathBuf};

use crate::{IconError, IconId, IconSource, SetInfo, SourcedIcon};

/// A folder of SVG files, one per icon: `<dir>/<set>/<name>.svg` is the icon `set:name`, so the application's own artwork is named and drawn exactly like a published set's.
///
/// A set is the application's own unless it says otherwise. One that holds third-party artwork says what it is in `<dir>/<set>/info.json`, written like the `info` of an Iconify set — `{ "name": …, "license": { "title": …, "spdx": …, "url": … } }` — and is then judged by the licence policy and listed in the notice like any other set.
pub struct SvgDir {
    root: PathBuf,
}

impl SvgDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn info(&self, prefix: &str) -> Result<Option<SetInfo>, IconError> {
        let path = self.root.join(prefix).join("info.json");
        if !path.is_file() {
            return Ok(None);
        }
        let failed = |message: String| IconError::Read {
            path: path.clone(),
            message,
        };
        let json = std::fs::read_to_string(&path).map_err(|e| failed(e.to_string()))?;
        serde_json::from_str(&json)
            .map(Some)
            .map_err(|e| failed(format!("not an Iconify set `info`: {e}")))
    }
}

impl IconSource for SvgDir {
    fn describe(&self) -> String {
        format!("the SVG folder `{}`", self.root.display())
    }

    fn icon(&self, id: &IconId) -> Result<Option<SourcedIcon>, IconError> {
        let path = self
            .root
            .join(id.prefix())
            .join(format!("{}.svg", id.name()));
        if !path.is_file() {
            return Ok(None);
        }
        let svg = std::fs::read_to_string(&path).map_err(|e| read_error(&path, e))?;
        let set = self.info(id.prefix())?;
        Ok(Some(SourcedIcon {
            own: set.is_none(),
            svg,
            set,
            origin: path.display().to_string(),
        }))
    }
}

fn read_error(path: &Path, error: std::io::Error) -> IconError {
    IconError::Read {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

#[cfg(test)]
#[path = "svg_dir_test.rs"]
mod tests;
