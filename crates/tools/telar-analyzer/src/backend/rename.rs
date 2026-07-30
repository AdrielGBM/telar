use std::path::PathBuf;

use lsp_types::*;
use ra_ap_ide::TextSize;

use crate::project::ProjectInfo;
use telar_transpiler::naming::{to_pascal_case, to_snake_case};

use super::Backend;

impl Backend {
    /// Renames a component (`<feature_card>` → `<new_name>`): the defining `.rsx` file, every markup usage (native cross-file scan), and every hand-written Rust reference to the generated `fn` / `Props` (via the embedded analyzer). Returns a `document_changes` edit so the file rename rides along with the text edits. `None` if the new name is not a valid identifier or no defining file is found. Cross-component bare-Rust calls to a *subdirectory* component aren't renamed (the tag model is file-stem-based; the generated fn name is the flattened path) — a documented limit.
    pub(crate) async fn rename_component(
        &self,
        old_name: &str,
        new_name: &str,
        uri: &Uri,
        theme: Option<String>,
    ) -> Option<WorkspaceEdit> {
        // The new name becomes a file stem + fn identifier, so it must be a bare identifier.
        let valid = !new_name.is_empty()
            && new_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !new_name.starts_with(|c: char| c.is_ascii_digit());
        if !valid {
            return None;
        }
        let path = crate::uri::to_path(uri)?;
        let root = telar_workspace::find_telar_root(&path)
            .or_else(|| telar_workspace::find_workspace_root(&path))?;

        // Markup usages + the defining file, from the workspace `.rsx` index.
        let old = old_name.to_string();
        let refs = self
            .with_index(root.clone(), move |idx| idx.component_references(&old))
            .await?;

        let mut edits: std::collections::HashMap<Uri, Vec<TextEdit>> =
            std::collections::HashMap::new();
        let mut def_uri: Option<Uri> = None;
        for loc in refs {
            // The (0,0) marker `component_references` emits for the defining file is the file itself, not a text occurrence — capture it for the rename op, never as an edit.
            if loc.range.start == loc.range.end {
                def_uri = Some(loc.uri);
            } else {
                edits.entry(loc.uri).or_default().push(TextEdit {
                    range: loc.range,
                    new_text: new_name.to_string(),
                });
            }
        }
        let def_uri = def_uri?;
        let def_path = crate::uri::to_path(&def_uri)?;

        // Hand-written Rust references to the generated `fn` / `Props` (in real `.rs` files).
        for (ru, redits) in self
            .component_rust_edits(
                def_path.clone(),
                old_name.to_string(),
                new_name.to_string(),
                theme,
            )
            .await
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
        let new_def_uri = crate::uri::from_path(&new_def_path)?;
        ops.push(DocumentChangeOperation::Op(ResourceOp::Rename(
            RenameFile {
                old_uri: def_uri,
                new_uri: new_def_uri,
                options: None,
                annotation_id: None,
            },
        )));

        Some(WorkspaceEdit {
            changes: None,
            document_changes: Some(DocumentChanges::Operations(ops)),
            change_annotations: None,
        })
    }

    /// Text edits in real `.rs` files for hand-written references to a component's generated `fn`/`Props` (e.g. `crate::feature_card(...)` / `crate::FeatureCardProps { .. }`). Queries the embedded analyzer from the generated definitions; generated build files are skipped (rebuilt from the `.rsx`). Returns empty if the analyzer isn't ready (logged) or the definitions aren't found.
    async fn component_rust_edits(
        &self,
        def_path: PathBuf,
        old_name: String,
        new_name: String,
        theme: Option<String>,
    ) -> std::collections::HashMap<Uri, Vec<TextEdit>> {
        let mut edits: std::collections::HashMap<Uri, Vec<TextEdit>> =
            std::collections::HashMap::new();
        let Some(root) = crate::build_sync::crate_root(&def_path) else {
            return edits;
        };
        let Ok(source) = std::fs::read_to_string(&def_path) else {
            return edits;
        };
        let def_theme = ProjectInfo::discover(&def_path)
            .and_then(|p| p.theme_type.clone())
            .or(theme);
        let Some(target) =
            crate::build_sync::generated_target(&def_path, &source, def_theme.as_deref())
        else {
            return edits;
        };

        let fn_name = to_snake_case(&old_name);
        let new_fn = to_snake_case(&new_name);
        let props_type = to_pascal_case(&old_name) + "Props";
        let new_props = to_pascal_case(&new_name) + "Props";

        // Offsets of the generated definition names (`fn NAME(` skips "fn "; `struct NAME` skips "struct ").
        let Some(fn_offset) = target.code.find(&format!("fn {fn_name}(")).map(|i| i + 3) else {
            return edits;
        };
        let props_offset = target
            .code
            .find(&format!("struct {props_type}"))
            .map(|i| i + 7);

        let gen_path = target.path.clone();
        let gen_code = target.code.clone();
        let result = self
            .run_analyzer(gen_path.clone(), root, move |a| {
                let fn_refs = a
                    .references_at_offset(
                        &gen_path,
                        gen_code.clone(),
                        TextSize::from(fn_offset as u32),
                    )
                    .unwrap_or_default();
                let props_refs = props_offset
                    .map(|o| {
                        a.references_at_offset(
                            &gen_path,
                            gen_code.clone(),
                            TextSize::from(o as u32),
                        )
                        .unwrap_or_default()
                    })
                    .unwrap_or_default();
                Some((fn_refs, props_refs))
            })
            .await;

        let Some((fn_refs, props_refs)) = result else {
            self.outgoing.log_message(
                MessageType::INFO,
                "telar-analyzer: analyzer not ready — component's Rust references were left unchanged"
                    .to_string(),
            );
            return edits;
        };

        for (refs, replacement) in [(fn_refs, new_fn.as_str()), (props_refs, new_props.as_str())] {
            for r in refs {
                // The generated module is rebuilt from the `.rsx`; only real source files need editing.
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
        edits
    }
}
