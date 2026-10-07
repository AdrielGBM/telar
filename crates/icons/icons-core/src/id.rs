//! [`IconId`]: an icon named the Iconify way, `set:name`.

use std::fmt;
use std::str::FromStr;

use crate::IconError;

/// An icon as Iconify names it: the prefix of its set and its name within that set, written `set:name` (`mdi:home`).
///
/// Both halves are lowercase ASCII letters and digits in runs joined by single hyphens, which is Iconify's own rule. It is also what lets a source map an id onto a file or a URL without escaping it: neither half can hold a separator, a dot or `..`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IconId {
    prefix: String,
    name: String,
}

impl IconId {
    /// Reads `set:name`.
    pub fn parse(id: &str) -> Result<Self, IconError> {
        let invalid = |reason| IconError::InvalidId {
            id: id.to_string(),
            reason,
        };
        let (prefix, name) = id
            .split_once(':')
            .ok_or_else(|| invalid("write it as `set:name`, like `mdi:home`"))?;
        Self::new(prefix, name).map_err(|error| match error {
            IconError::InvalidId { reason, .. } => invalid(reason),
            other => other,
        })
    }

    /// Reads `set/name`, the layout a provider URL and a folder of SVGs use, and the id a runtime transport is handed.
    pub fn from_path(path: &str) -> Result<Self, IconError> {
        let (prefix, name) = path.split_once('/').ok_or(IconError::InvalidId {
            id: path.to_string(),
            reason: "write it as `set/name`, like `mdi/home`",
        })?;
        Self::new(prefix, name)
    }

    /// The id of `name` in the set `prefix`.
    pub fn new(prefix: &str, name: &str) -> Result<Self, IconError> {
        let invalid = |reason| IconError::InvalidId {
            id: format!("{prefix}:{name}"),
            reason,
        };
        if !is_iconify_word(prefix) {
            return Err(invalid(
                "the set is lowercase letters and digits joined by single hyphens, like `mdi` or `material-symbols`",
            ));
        }
        if !is_iconify_word(name) {
            return Err(invalid(
                "the name is lowercase letters and digits joined by single hyphens, like `home` or `arrow-left`",
            ));
        }
        Ok(Self {
            prefix: prefix.to_string(),
            name: name.to_string(),
        })
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// `set/name`.
    pub fn path(&self) -> String {
        format!("{}/{}", self.prefix, self.name)
    }
}

impl fmt::Display for IconId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.prefix, self.name)
    }
}

impl FromStr for IconId {
    type Err = IconError;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        Self::parse(id)
    }
}

fn is_iconify_word(word: &str) -> bool {
    !word.is_empty()
        && word.split('-').all(|run| {
            !run.is_empty()
                && run
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

#[cfg(test)]
#[path = "id_test.rs"]
mod tests;
