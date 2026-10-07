//! [`IconifyDir`]: Iconify JSON sets read from a directory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::{IconError, IconId, IconSet, IconSource, SourcedIcon};

/// Iconify JSON sets in a directory, in any of the layouts they are installed or downloaded in. For the set `mdi`, the first of these that exists:
///
/// - `<dir>/mdi.json`, a set saved on its own;
/// - `<dir>/json/mdi.json`, the `@iconify/json` package itself;
/// - `<dir>/@iconify-json/mdi/icons.json`, one `@iconify-json/mdi` package, with `<dir>` being `node_modules`;
/// - `<dir>/@iconify/json/json/mdi.json`, the whole `@iconify/json` package installed under `node_modules`.
///
/// A set is parsed the first time an icon of it is asked for and kept for the life of the source, so a bake reads each set once however many of its icons the application draws.
pub struct IconifyDir {
    root: PathBuf,
    sets: Mutex<HashMap<String, Option<Loaded>>>,
}

#[derive(Clone)]
struct Loaded {
    set: Arc<IconSet>,
    path: PathBuf,
}

impl IconifyDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            sets: Mutex::new(HashMap::new()),
        }
    }

    /// The set `prefix`, or `None` when no layout above holds it.
    pub fn set(&self, prefix: &str) -> Result<Option<Arc<IconSet>>, IconError> {
        Ok(self.loaded(prefix)?.map(|loaded| loaded.set))
    }

    fn loaded(&self, prefix: &str) -> Result<Option<Loaded>, IconError> {
        let mut sets = self.sets.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = sets.get(prefix) {
            return Ok(cached.clone());
        }
        let loaded = match self
            .candidates(prefix)
            .into_iter()
            .find(|path| path.is_file())
        {
            Some(path) => Some(Loaded {
                set: Arc::new(read_set(&path, prefix)?),
                path,
            }),
            None => None,
        };
        sets.insert(prefix.to_string(), loaded.clone());
        Ok(loaded)
    }

    fn candidates(&self, prefix: &str) -> [PathBuf; 4] {
        let file = format!("{prefix}.json");
        [
            self.root.join(&file),
            self.root.join("json").join(&file),
            self.root
                .join("@iconify-json")
                .join(prefix)
                .join("icons.json"),
            self.root
                .join("@iconify")
                .join("json")
                .join("json")
                .join(&file),
        ]
    }
}

fn read_set(path: &Path, prefix: &str) -> Result<IconSet, IconError> {
    let failed = |message: String| IconError::Read {
        path: path.to_path_buf(),
        message,
    };
    let json = std::fs::read_to_string(path).map_err(|e| failed(e.to_string()))?;
    let set =
        IconSet::from_json(&json).map_err(|e| failed(format!("not an Iconify icon set: {e}")))?;
    if set.prefix != prefix {
        return Err(failed(format!(
            "holds the set `{}`, where the file name says `{prefix}`",
            set.prefix
        )));
    }
    Ok(set)
}

impl IconSource for IconifyDir {
    fn describe(&self) -> String {
        format!("the Iconify sets in `{}`", self.root.display())
    }

    fn icon(&self, id: &IconId) -> Result<Option<SourcedIcon>, IconError> {
        let Some(Loaded { set, path }) = self.loaded(id.prefix())? else {
            return Ok(None);
        };
        Ok(set.icon(id.name()).map(|icon| SourcedIcon {
            svg: icon.to_svg(),
            set: set.info.clone(),
            own: false,
            origin: path.display().to_string(),
        }))
    }
}

#[cfg(test)]
#[path = "iconify_dir_test.rs"]
mod tests;
