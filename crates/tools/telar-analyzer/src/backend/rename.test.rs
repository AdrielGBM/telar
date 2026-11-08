use super::*;

use std::sync::Arc;

use lsp_server::{ErrorCode, Request, Response};
use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::backend::AnalyzerState;
use crate::inner::fake;
use crate::ra::Analyzer;
use crate::rpc::OutgoingSender;

/// A telar crate where `home` uses the `feature_card` component, and hand-written Rust sits beside them.
struct Project(PathBuf);

impl Project {
    fn create(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("telar-rename-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"probe\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::write(root.join(telar_project::MANIFEST_FILENAME), "[telar]\n").unwrap();
        std::fs::write(
            root.join("src/feature_card.rsx"),
            "[view]\ncol\n    text \"hi\"\n",
        )
        .unwrap();
        std::fs::write(root.join("src/home.rsx"), "[view]\ncol\n    feature_card\n").unwrap();
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn shown() { feature_card(); }\n",
        )
        .unwrap();
        Self(root)
    }

    fn uri(&self, relative: &str) -> Uri {
        crate::uri::from_path(&self.0.join(relative)).unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn backend_with(state: AnalyzerState) -> (Backend, UnboundedReceiver<Value>) {
    let (tx, editor) = tokio::sync::mpsc::unbounded_channel();
    let backend = Backend::new(OutgoingSender::new(tx));
    *backend.analyzer.state.lock().unwrap() = state;
    (backend, editor)
}

fn session_answering(answer: impl Fn(Request) -> Response + Send + 'static) -> AnalyzerState {
    let (inner, _editor) = fake::session(fake::answering(answer));
    AnalyzerState::Ready(Arc::new(Analyzer::from(inner)))
}

fn shown_messages(editor: &mut UnboundedReceiver<Value>) -> Vec<String> {
    let mut shown = Vec::new();
    while let Ok(message) = editor.try_recv() {
        if message["method"] == "window/showMessage" {
            shown.push(message["params"]["message"].as_str().unwrap().to_owned());
        }
    }
    shown
}

fn text_edits_in(edit: &WorkspaceEdit, uri: &Uri) -> Vec<String> {
    let Some(DocumentChanges::Operations(ops)) = &edit.document_changes else {
        return Vec::new();
    };
    ops.iter()
        .filter_map(|op| match op {
            DocumentChangeOperation::Edit(edit) if &edit.text_document.uri == uri => {
                Some(edit.edits.iter())
            }
            _ => None,
        })
        .flatten()
        .filter_map(|edit| match edit {
            OneOf::Left(edit) => Some(edit.new_text.clone()),
            OneOf::Right(_) => None,
        })
        .collect()
}

#[tokio::test]
async fn a_component_rename_is_refused_when_rust_analyzer_cannot_list_its_rust_references() {
    let project = Project::create("answered-error");
    let (backend, mut editor) = backend_with(session_answering(|request| {
        Response::new_err(
            request.id,
            ErrorCode::RequestFailed as i32,
            "no crate graph yet".to_owned(),
        )
    }));

    let renamed = backend
        .rename_component(
            "feature_card",
            "hero_card",
            &project.uri("src/home.rsx"),
            None,
        )
        .await;

    let refused = renamed.expect_err("a rename that cannot see its Rust references must not apply");
    assert!(
        refused.message().contains("no crate graph yet"),
        "the refusal should carry rust-analyzer's reason: {}",
        refused.message()
    );
    assert_eq!(
        shown_messages(&mut editor),
        vec![refused.message().to_owned()],
        "the user is told the same reason"
    );
}

#[tokio::test]
async fn a_component_rename_is_refused_while_no_rust_analyzer_session_serves_the_crate() {
    let project = Project::create("no-session");
    let (backend, mut editor) = backend_with(AnalyzerState::Failed);

    let renamed = backend
        .rename_component(
            "feature_card",
            "hero_card",
            &project.uri("src/home.rsx"),
            None,
        )
        .await;

    assert!(renamed.is_err(), "the .rsx half alone must not be applied");
    assert_eq!(shown_messages(&mut editor).len(), 1);
}

#[tokio::test]
async fn a_component_rename_edits_the_rust_references_rust_analyzer_lists() {
    let project = Project::create("answered");
    let lib_rs = project.uri("src/lib.rs");
    let (backend, mut editor) = backend_with(session_answering({
        let lib_rs = lib_rs.clone();
        move |request| {
            let generated = request.params["textDocument"]["uri"].clone();
            let span = json!({ "start": { "line": 0, "character": 17 }, "end": { "line": 0, "character": 29 } });
            Response::new_ok(
                request.id,
                json!([
                    { "uri": generated, "range": span },
                    { "uri": lib_rs.as_str(), "range": span },
                ]),
            )
        }
    }));

    let edit = backend
        .rename_component(
            "feature_card",
            "hero_card",
            &project.uri("src/home.rsx"),
            None,
        )
        .await
        .expect("rust-analyzer listed every reference")
        .expect("the component was found");

    assert!(
        text_edits_in(&edit, &lib_rs).contains(&"hero_card".to_owned()),
        "the hand-written Rust call is renamed"
    );
    assert_eq!(
        text_edits_in(&edit, &project.uri("src/home.rsx")),
        vec!["hero_card".to_owned()],
        "the markup usage is renamed"
    );
    assert!(shown_messages(&mut editor).is_empty());
}

fn renamed_files(edit: &WorkspaceEdit) -> Vec<(Uri, Uri)> {
    let Some(DocumentChanges::Operations(ops)) = &edit.document_changes else {
        return Vec::new();
    };
    ops.iter()
        .filter_map(|op| match op {
            DocumentChangeOperation::Op(ResourceOp::Rename(rename)) => {
                Some((rename.old_uri.clone(), rename.new_uri.clone()))
            }
            _ => None,
        })
        .collect()
}

fn listing_only_the_generated_definition() -> AnalyzerState {
    session_answering(|request| {
        let generated = request.params["textDocument"]["uri"].clone();
        let span = json!({ "start": { "line": 0, "character": 17 }, "end": { "line": 0, "character": 29 } });
        Response::new_ok(request.id, json!([{ "uri": generated, "range": span }]))
    })
}

#[tokio::test]
async fn a_component_rename_moves_the_defining_file_and_its_preview_file() {
    let project = Project::create("files-move");
    std::fs::write(
        project.0.join("src/feature_card.preview.rsx"),
        "[view]\nfeature_card\n",
    )
    .unwrap();
    let (backend, _editor) = backend_with(listing_only_the_generated_definition());

    let edit = backend
        .rename_component(
            "feature_card",
            "hero_card",
            &project.uri("src/home.rsx"),
            None,
        )
        .await
        .expect("rust-analyzer listed every reference")
        .expect("the component was found");

    let mut moves = renamed_files(&edit);
    moves.sort_by_key(|(old, _)| old.as_str().to_owned());
    assert_eq!(
        moves,
        vec![
            (
                project.uri("src/feature_card.preview.rsx"),
                project.uri("src/hero_card.preview.rsx")
            ),
            (
                project.uri("src/feature_card.rsx"),
                project.uri("src/hero_card.rsx")
            ),
        ]
    );
}

#[tokio::test]
async fn a_component_rename_to_something_that_is_not_an_identifier_is_declined() {
    let project = Project::create("bad-name");
    let (backend, mut editor) = backend_with(listing_only_the_generated_definition());

    for new_name in ["", "9lives", "hero-card", "hero card", "héroe"] {
        let renamed = backend
            .rename_component("feature_card", new_name, &project.uri("src/home.rsx"), None)
            .await;
        assert!(matches!(renamed, Ok(None)), "{new_name:?}");
    }
    assert!(shown_messages(&mut editor).is_empty());
}

#[tokio::test]
async fn a_component_nobody_defines_is_declined_without_asking_rust_analyzer() {
    let project = Project::create("unknown");
    let (backend, mut editor) = backend_with(AnalyzerState::Failed);

    let renamed = backend
        .rename_component(
            "missing_card",
            "hero_card",
            &project.uri("src/home.rsx"),
            None,
        )
        .await;

    assert!(matches!(renamed, Ok(None)));
    assert!(shown_messages(&mut editor).is_empty());
}
