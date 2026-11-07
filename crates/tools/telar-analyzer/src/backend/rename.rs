//! Renaming a component: its file, its markup usages and its Rust references, in one workspace edit.

use std::path::PathBuf;

use lsp_types::*;

use crate::project::ProjectInfo;
use telar_project::naming::{to_pascal_case, to_snake_case};

use super::query::Unresolved;
use super::{Backend, RenameRefused};

impl Backend {
    /// Renames a component (`<feature_card>` → `<new_name>`): the defining `.rsx` file and any `*.preview.rsx` named after it, every markup usage (native cross-file scan), and every hand-written Rust reference to the generated `fn` / `Props` (via the embedded analyzer). Returns a `document_changes` edit so the file rename rides along with the text edits. `None` if the new name is not a valid identifier or no defining file is found; refused, with nothing edited, when the Rust references cannot be listed.
    ///
    /// Three kinds of occurrence, because a component name lives in three places: the tag in `[view]`, the module segment of the `use` line that imports it (both from the workspace `.rsx` index — the segment is a file name, which rust-analyzer never sees change), and the generated `fn`/`Props` in real Rust, which is rust-analyzer's own rename mapped back through the source map.
    pub(crate) async fn rename_component(
        &self,
        old_name: &str,
        new_name: &str,
        uri: &Uri,
        theme: Option<String>,
    ) -> Result<Option<WorkspaceEdit>, RenameRefused> {
        // The new name becomes a file stem and fn identifier, so it must be a bare identifier.
        let valid = !new_name.is_empty()
            && new_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !new_name.starts_with(|c: char| c.is_ascii_digit());
        if !valid {
            return Ok(None);
        }
        let Some(root) = crate::uri::to_path(uri).and_then(|path| {
            telar_project::find_telar_root(&path)
                .or_else(|| telar_project::find_workspace_root(&path))
        }) else {
            return Ok(None);
        };

        let old = old_name.to_string();
        let Some((refs, preview_files)) = self
            .with_index(root.clone(), move |idx| {
                (idx.component_references(&old), idx.preview_files(&old))
            })
            .await
        else {
            return Ok(None);
        };

        // `lsp_types::Uri` hashes and compares by `as_str()` alone; the `Cell` inside fluent-uri's parse data is an authority offset that takes no part in either.
        #[allow(clippy::mutable_key_type)]
        let mut edits: std::collections::HashMap<Uri, Vec<TextEdit>> =
            std::collections::HashMap::new();
        let mut def_uri: Option<Uri> = None;
        for loc in refs {
            // The (0,0) marker `component_references` emits for the defining file is the file itself, not a text occurrence, so it is captured for the rename op and never as an edit.
            if loc.range.start == loc.range.end {
                def_uri = Some(loc.uri);
            } else {
                edits.entry(loc.uri).or_default().push(TextEdit {
                    range: loc.range,
                    new_text: new_name.to_string(),
                });
            }
        }
        let Some((def_uri, def_path)) =
            def_uri.and_then(|uri| crate::uri::to_path(&uri).map(|path| (uri, path)))
        else {
            return Ok(None);
        };

        for (ru, redits) in self
            .component_rust_edits(
                def_path.clone(),
                old_name.to_string(),
                new_name.to_string(),
                theme,
            )
            .await
            .map_err(|why| {
                self.rename_refused(&format!(
                    "the Rust references to `{old_name}` could not be listed: {why}. Renaming only its .rsx would leave them naming the old name, so nothing was changed."
                ))
            })?
        {
            edits.entry(ru).or_default().extend(redits);
        }

        // Text edits first, then the file rename, so edits against the old URI apply before it moves.
        let mut ops: Vec<DocumentChangeOperation> = edits
            .into_iter()
            .map(|(uri, edits)| {
                DocumentChangeOperation::Edit(TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier { uri, version: None },
                    edits: edits.into_iter().map(OneOf::Left).collect(),
                })
            })
            .collect();
        let new_def_path = def_path.with_file_name(format!("{new_name}.rsx"));
        let Some(new_def_uri) = crate::uri::from_path(&new_def_path) else {
            return Ok(None);
        };
        ops.push(rename_file(def_uri, new_def_uri));
        // A preview file is named after the component it previews, which is what its ids and its entry table are named by.
        for old_uri in preview_files {
            let Some(old_path) = crate::uri::to_path(&old_uri) else {
                continue;
            };
            let new_path = old_path.with_file_name(format!("{new_name}.preview.rsx"));
            if let Some(new_uri) = crate::uri::from_path(&new_path) {
                ops.push(rename_file(old_uri, new_uri));
            }
        }

        Ok(Some(WorkspaceEdit {
            changes: None,
            document_changes: Some(DocumentChanges::Operations(ops)),
            change_annotations: None,
        }))
    }

    /// Text edits in real `.rs` files for hand-written references to a component's generated `fn`/`Props` (e.g. `crate::feature_card(...)` / `crate::FeatureCardProps { .. }`). Queries the embedded analyzer from the generated definitions; generated build files are skipped (rebuilt from the `.rsx`). An error when the definitions cannot be generated or found, or rust-analyzer cannot list their references, since an empty answer would read as "nothing to edit".
    async fn component_rust_edits(
        &self,
        def_path: PathBuf,
        old_name: String,
        new_name: String,
        theme: Option<String>,
    ) -> Result<std::collections::HashMap<Uri, Vec<TextEdit>>, Unresolved> {
        let root = crate::build_sync::crate_root(&def_path).ok_or(Unresolved::NotAsked)?;
        let source = std::fs::read_to_string(&def_path).map_err(|_| Unresolved::NotAsked)?;
        let def_theme = ProjectInfo::discover(&def_path)
            .and_then(|p| p.theme_type.clone())
            .or(theme);
        let target = crate::build_sync::generated_target(&def_path, &source, def_theme.as_deref())
            .ok_or(Unresolved::NotAsked)?;

        let fn_name = to_snake_case(&old_name);
        let new_fn = to_snake_case(&new_name);
        let props_type = to_pascal_case(&old_name) + "Props";
        let new_props = to_pascal_case(&new_name) + "Props";

        // `fn NAME(` skips "fn "; `struct NAME` skips "struct ".
        let fn_offset = target
            .code
            .find(&format!("fn {fn_name}("))
            .map(|i| i + 3)
            .ok_or(Unresolved::NotAsked)?;
        let props_offset = target
            .code
            .find(&format!("struct {props_type}"))
            .map(|i| i + 7);

        let gen_path = target.path.clone();
        let gen_code = target.code.clone();
        let (fn_refs, props_refs) = self
            .run_analyzer(root, move |a| async move {
                let fn_refs = a.references(&gen_path, &gen_code, fn_offset).await?;
                let props_refs = match props_offset {
                    Some(offset) => a.references(&gen_path, &gen_code, offset).await?,
                    None => Vec::new(),
                };
                Ok::<_, Unresolved>((fn_refs, props_refs))
            })
            .await
            .ok_or(Unresolved::NotAsked)??;

        // `lsp_types::Uri` hashes and compares by `as_str()` alone; the `Cell` inside fluent-uri's parse data is an authority offset that takes no part in either.
        #[allow(clippy::mutable_key_type)]
        let mut edits: std::collections::HashMap<Uri, Vec<TextEdit>> =
            std::collections::HashMap::new();
        for (refs, replacement) in [(fn_refs, new_fn.as_str()), (props_refs, new_props.as_str())] {
            for r in refs {
                // The generated module is rebuilt from the `.rsx`, so only real source files need editing.
                if crate::build_sync::is_generated_build_file(&r.path) {
                    continue;
                }
                let Some(uri) = crate::uri::from_path(&r.path) else {
                    continue;
                };
                edits.entry(uri).or_default().push(TextEdit {
                    range: r.range,
                    new_text: replacement.to_string(),
                });
            }
        }
        Ok(edits)
    }
}

fn rename_file(old_uri: Uri, new_uri: Uri) -> DocumentChangeOperation {
    DocumentChangeOperation::Op(ResourceOp::Rename(RenameFile {
        old_uri,
        new_uri,
        options: None,
        annotation_id: None,
    }))
}

#[cfg(test)]
#[path = "rename.test.rs"]
mod tests;
