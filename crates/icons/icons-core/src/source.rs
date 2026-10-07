//! [`IconSource`]: where an id's SVG comes from, as one seam the baker and the runtime both resolve through.

use crate::{IconError, IconId, SetInfo};

/// One resolved icon: a standalone SVG document, and what is known about the set it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct SourcedIcon {
    pub svg: String,
    /// What the set says about itself, its licence included. `None` when the source has nothing to say, such as an SVG folder with no `info.json`.
    pub set: Option<SetInfo>,
    /// `true` for the application's own artwork rather than a published set: an [`SvgDir`](crate::SvgDir) set that declares no `info.json`. The licence policy does not judge it.
    pub own: bool,
    /// Where the SVG was read, a file or a URL, for diagnostics and the licence notice.
    pub origin: String,
}

/// Resolves icon ids into SVG documents.
///
/// `Ok(None)` means this source does not have the icon, which is not a failure: [`Sources`] asks the next one. `Err` means the source itself is broken — a set file that does not parse, a provider that does not answer — and is reported rather than skipped, because falling through would bake whatever the next source happens to hold under that name.
///
/// `Send + Sync` because one source is shared by every resolution a bake or a runtime transport starts, from whatever thread starts it.
pub trait IconSource: Send + Sync {
    /// What this source is, for a message that has to name it: "the Iconify sets in `node_modules`".
    fn describe(&self) -> String;

    fn icon(&self, id: &IconId) -> Result<Option<SourcedIcon>, IconError>;

    /// Resolves every id in `ids`, answering in the same order. The default asks one at a time; a source that can answer many in one request, such as a provider, overrides it.
    fn icons(&self, ids: &[IconId]) -> Vec<Result<Option<SourcedIcon>, IconError>> {
        ids.iter().map(|id| self.icon(id)).collect()
    }
}

/// Several sources asked in order: the first that has an icon answers for it.
#[derive(Default)]
pub struct Sources {
    sources: Vec<Box<dyn IconSource>>,
}

impl Sources {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks `source` after every source already added.
    pub fn with(mut self, source: impl IconSource + 'static) -> Self {
        self.sources.push(Box::new(source));
        self
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

impl IconSource for Sources {
    fn describe(&self) -> String {
        match self.sources.as_slice() {
            [] => "no icon source".to_string(),
            sources => sources
                .iter()
                .map(|source| source.describe())
                .collect::<Vec<_>>()
                .join(", then "),
        }
    }

    fn icon(&self, id: &IconId) -> Result<Option<SourcedIcon>, IconError> {
        for source in &self.sources {
            if let Some(icon) = source.icon(id)? {
                return Ok(Some(icon));
            }
        }
        Ok(None)
    }

    /// Each source is handed only the ids every earlier one lacked, in one call, so a provider at the end of the chain still answers a whole set in one request.
    fn icons(&self, ids: &[IconId]) -> Vec<Result<Option<SourcedIcon>, IconError>> {
        let mut answers: Vec<Result<Option<SourcedIcon>, IconError>> =
            ids.iter().map(|_| Ok(None)).collect();
        for source in &self.sources {
            let open: Vec<usize> = (0..ids.len())
                .filter(|&i| matches!(answers[i], Ok(None)))
                .collect();
            if open.is_empty() {
                break;
            }
            let asked: Vec<IconId> = open.iter().map(|&i| ids[i].clone()).collect();
            for (i, answer) in open.into_iter().zip(source.icons(&asked)) {
                answers[i] = answer;
            }
        }
        answers
    }
}

#[cfg(test)]
#[path = "source_test.rs"]
mod tests;
