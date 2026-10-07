//! What a module exports, asked of rust-analyzer the one way it answers for any path, globs and re-exports included: by completing `<path>::` inside the package, the way an editor would.

use std::path::Path;

use lsp_types::CompletionItem;
use serde_json::{Value, json};

use super::queries::completion_list;
use super::{Analyzer, QUERY_TIMEOUT};

impl Analyzer {
    /// What each of `paths` exports, as rust-analyzer completes `<path>::` from inside `host`, narrowed by `keep` and resolved so every kept item carries its detail and documentation. `None` for a path it had nothing for: no answer in time, or no items, which is also what it answers while the workspace is still loading.
    ///
    /// The question is a temporary edit of `host`: probe functions appended to whatever rust-analyzer holds for it (`fallback` when it holds nothing), taken back once answered. A query that synced the file in the meantime holds the newer text, so it is left in place.
    pub async fn path_exports(
        &self,
        host: &Path,
        fallback: &str,
        paths: &[String],
        keep: impl Fn(&[CompletionItem]) -> Vec<CompletionItem>,
    ) -> Vec<Option<Vec<CompletionItem>>> {
        let base = self
            .inner
            .synced_text(host)
            .unwrap_or_else(|| fallback.to_owned());
        let (probe, offsets) = probe_text(&base, paths);
        let uri = crate::inner::uri_for(host);

        self.inner.sync(host, &probe);
        let params = offsets
            .iter()
            .map(|&offset| {
                json!({
                    "textDocument": { "uri": uri },
                    "position": crate::text::offset_to_position(&probe, offset),
                })
            })
            .collect();
        let answers = self
            .inner
            .request_all("textDocument/completion", params, QUERY_TIMEOUT)
            .await;
        let mut exports: Vec<Option<Vec<CompletionItem>>> = answers
            .into_iter()
            .map(|answer| {
                let items = completion_list(answer?);
                (!items.is_empty()).then(|| keep(&items))
            })
            .collect();

        // rust-analyzer resolves an item by completing again at the position it was offered from, so the probe has to be what it holds while it does.
        self.inner.sync(host, &probe);
        self.resolve_all(&mut exports).await;

        if self.inner.synced_text(host).as_deref() == Some(probe.as_str()) {
            self.inner.sync(host, &base);
        }
        exports
    }

    /// Fills in the detail and documentation a client that resolves lazily is first sent without. An item rust-analyzer cannot resolve keeps what it had.
    async fn resolve_all(&self, exports: &mut [Option<Vec<CompletionItem>>]) {
        let unresolved: Vec<&mut CompletionItem> = exports
            .iter_mut()
            .flatten()
            .flatten()
            .filter(|item| item.data.is_some())
            .collect();
        let params: Vec<Value> = unresolved
            .iter()
            .filter_map(|item| serde_json::to_value(&**item).ok())
            .collect();
        if params.len() != unresolved.len() {
            return;
        }
        let answers = self
            .inner
            .request_all("completionItem/resolve", params, QUERY_TIMEOUT)
            .await;
        for (item, answer) in unresolved.into_iter().zip(answers) {
            if let Some(resolved) =
                answer.and_then(|value| serde_json::from_value::<CompletionItem>(value).ok())
            {
                item.detail = resolved.detail.or(item.detail.take());
                item.documentation = resolved.documentation.or(item.documentation.take());
            }
        }
    }
}

/// `base` with one probe function per path, each ending its body on `<path>::`, and the byte offset right after each of those `::`.
fn probe_text(base: &str, paths: &[String]) -> (String, Vec<usize>) {
    let mut text = base.to_owned();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    let mut offsets = Vec::with_capacity(paths.len());
    for (index, path) in paths.iter().enumerate() {
        let root = path.split("::").next().unwrap_or_default();
        // A crate name needs the leading `::` to mean the extern crate wherever `host` sits; `crate`, `self` and `super` already say where they start.
        let anchor = match matches!(root, "crate" | "self" | "super") {
            true => "",
            false => "::",
        };
        text.push_str(&format!(
            "#[allow(dead_code)]\nfn __telar_exports_{index}() {{ {anchor}{path}::"
        ));
        offsets.push(text.len());
        text.push_str(" }\n");
    }
    (text, offsets)
}

#[cfg(test)]
#[path = "exports_test.rs"]
mod tests;
