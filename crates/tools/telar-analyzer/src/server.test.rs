use super::*;

fn routed(method: &str, params: Value) -> Route {
    route(method, &params)
}

fn on_document(method: &str, uri: &str) -> Route {
    routed(method, json!({ "textDocument": { "uri": uri } }))
}

#[test]
fn lifecycle_and_workspace_wide_requests_are_ours_whatever_they_carry() {
    for method in [
        "initialize",
        "shutdown",
        "workspace/symbol",
        PREVIEW_AT,
        WORKSHOPS,
    ] {
        assert_eq!(routed(method, Value::Null), Route::Ours, "{method}");
        assert_eq!(
            on_document(method, "file:///app/src/lib.rs"),
            Route::Ours,
            "{method} over a Rust file"
        );
    }
}

#[test]
fn initialized_and_watched_file_changes_reach_both_sides() {
    for method in ["initialized", "workspace/didChangeWatchedFiles"] {
        assert_eq!(routed(method, Value::Null), Route::Both, "{method}");
    }
}

#[test]
fn document_methods_over_an_rsx_file_are_ours() {
    for method in [
        "textDocument/didOpen",
        "textDocument/didChange",
        "textDocument/didClose",
        "textDocument/completion",
        "textDocument/hover",
        "textDocument/definition",
        "textDocument/formatting",
        "textDocument/rename",
        "textDocument/semanticTokens/full",
    ] {
        assert_eq!(
            on_document(method, "file:///app/src/home.rsx"),
            Route::Ours,
            "{method}"
        );
    }
}

#[test]
fn document_methods_over_real_rust_travel_to_rust_analyzer() {
    for method in [
        "textDocument/didOpen",
        "textDocument/completion",
        "textDocument/hover",
        "textDocument/definition",
        "textDocument/rename",
        "textDocument/inlayHint",
    ] {
        assert_eq!(
            on_document(method, "file:///app/src/lib.rs"),
            Route::Passthrough,
            "{method}"
        );
    }
}

#[test]
fn a_method_without_a_document_goes_to_rust_analyzer() {
    assert_eq!(
        routed("workspace/executeCommand", json!({})),
        Route::Passthrough
    );
    assert_eq!(
        routed("rust-analyzer/analyzerStatus", Value::Null),
        Route::Passthrough
    );
    assert_eq!(
        routed("textDocument/hover", json!({ "textDocument": {} })),
        Route::Passthrough
    );
}

#[test]
fn a_completion_item_resolves_where_it_came_from() {
    assert_eq!(
        routed(
            "completionItem/resolve",
            json!({ "label": "col", "data": { "g": 3 } })
        ),
        Route::Ours
    );
    assert_eq!(
        routed(
            "completionItem/resolve",
            json!({ "label": "fn", "data": { "other": 1 } })
        ),
        Route::Passthrough
    );
    assert_eq!(
        routed("completionItem/resolve", json!({ "label": "fn" })),
        Route::Passthrough
    );
}
