use lsp_types::Uri;
use rsx_diagnostics::Diagnostic;
use rsx_parser::{RsxDocument, parse};
use std::collections::HashMap;

pub struct ParsedDocument {
    pub source: String,
    pub document: RsxDocument,
}

pub struct Store {
    docs: HashMap<Uri, ParsedDocument>,
    // The current buffer text, updated on every edit even when it fails to parse. `docs` only holds
    // the last good parse (so completion/hover keep working mid-edit), but formatting must operate on
    // the live text — otherwise it could reformat a stale version and clobber the user's edits.
    latest_source: HashMap<Uri, String>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            docs: HashMap::new(),
            latest_source: HashMap::new(),
        }
    }

    pub fn close(&mut self, uri: &Uri) {
        self.docs.remove(uri);
        self.latest_source.remove(uri);
    }

    pub fn get(&self, uri: &Uri) -> Option<&ParsedDocument> {
        self.docs.get(uri)
    }

    /// The latest buffer text for `uri`, regardless of whether it currently parses.
    pub fn latest_source(&self, uri: &Uri) -> Option<&String> {
        self.latest_source.get(uri)
    }

    pub fn reparse(&mut self, uri: Uri, source: String) -> Vec<Diagnostic> {
        self.latest_source.insert(uri.clone(), source.clone());
        match parse(&source) {
            Ok(document) => {
                self.docs.insert(uri, ParsedDocument { source, document });
                vec![]
            }
            Err(err) => vec![Diagnostic::from(err)],
        }
    }
}
