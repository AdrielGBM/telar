//! The `.rsx` side's view of rust-analyzer: every query is put to the in-process server in [`crate::inner`], and always against the generated Rust rather than the source the editor is showing.
//!
//! Position mapping (`.rsx` cursor ↔ generated `.rs`) stays our responsibility via the transpiler's source map. rust-analyzer is told nothing about `.rsx`; the generated file reaches it as an ordinary `#[path] mod`, and a query is a `didChange` carrying the live transpile followed by a request at a position inside it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use lsp_types::{InlayHintKind, Range};
use serde_json::Value;

use crate::inner::Inner;
use crate::rpc::OutgoingSender;

mod queries;

/// How long a query waits for rust-analyzer. Past this it is either still loading the workspace or wedged, and either way the backend answers with what the `.rsx` side worked out natively rather than holding the editor.
const QUERY_TIMEOUT: Duration = Duration::from_secs(10);

/// A go-to-definition target: the target file's path and the name range within it. The backend decides whether the path is a generated `.telar/build/*.rs` (reverse-mapped to the `.rsx`) or a real file (used verbatim).
pub struct DefinitionTarget {
    pub path: PathBuf,
    pub range: Range,
}

/// One reference to the symbol under the cursor, declaration site included. The backend reverse-maps generated `.telar/build/*.rs` paths back onto their `.rsx` and uses real source paths verbatim.
pub struct RefTarget {
    pub path: PathBuf,
    pub range: Range,
}

/// One inlay hint, anchored in generated-file coordinates. The backend reverse-maps it onto the `.rsx` and keeps only `[logic]`-origin hints.
pub struct InlayHintRaw {
    pub line: u32,
    pub col: u32,
    pub pad_left: bool,
    pub pad_right: bool,
    pub kind: Option<InlayHintKind>,
    pub label: String,
}

/// The workspace's rust-analyzer, wrapped in the `.rsx`-shaped queries the backend asks.
pub struct Analyzer {
    inner: Inner,
}

impl Analyzer {
    /// Boots rust-analyzer over the workspace at `root`, handing it the editor's own `initializationOptions` and capabilities. Blocking; callers run it off the runtime thread.
    pub fn start(
        root: &Path,
        options: Value,
        editor_caps: Value,
        outgoing: OutgoingSender,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            inner: Inner::start(root, options, editor_caps, outgoing)?,
        })
    }

    pub fn capabilities(&self) -> &Value {
        self.inner.capabilities()
    }

    pub fn pass_request(&self, id: Value, method: &str, params: Value) {
        self.inner.pass_request(id, method, params);
    }

    pub fn pass_notification(&self, method: &str, params: Value) {
        self.inner.notify(method, params);
    }

    pub fn relay_reply(&self, id: i32, result: Option<Value>, error: Option<Value>) {
        self.inner.relay_reply(id, result, error);
    }
}
