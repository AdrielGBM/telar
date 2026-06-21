use rsx_parser::{RsxDocument, parse};
use std::collections::HashMap;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range, Url};

pub struct ParsedDocument {
    pub source: String,
    pub document: RsxDocument,
}

pub struct Store {
    docs: HashMap<Url, ParsedDocument>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            docs: HashMap::new(),
        }
    }

    pub fn open(&mut self, uri: Url, source: String) -> Vec<Diagnostic> {
        self.reparse(uri, source)
    }

    pub fn change(&mut self, uri: Url, source: String) -> Vec<Diagnostic> {
        self.reparse(uri, source)
    }

    pub fn close(&mut self, uri: &Url) {
        self.docs.remove(uri);
    }

    pub fn get(&self, uri: &Url) -> Option<&ParsedDocument> {
        self.docs.get(uri)
    }

    fn reparse(&mut self, uri: Url, source: String) -> Vec<Diagnostic> {
        match parse(&source) {
            Ok(document) => {
                self.docs.insert(uri, ParsedDocument { source, document });
                vec![]
            }
            Err(err) => {
                // convert 1-based parser line to 0-based LSP line
                let line = err.line.saturating_sub(1) as u32;
                let diagnostic = Diagnostic {
                    range: Range {
                        start: Position { line, character: 0 },
                        end: Position {
                            line,
                            character: u32::MAX,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: err.message,
                    ..Default::default()
                };
                vec![diagnostic]
            }
        }
    }
}
