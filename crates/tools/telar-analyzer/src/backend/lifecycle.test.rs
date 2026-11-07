use super::*;

use std::time::Duration;

use lsp_server::Message;
use serde_json::json;

use crate::inner::fake;
use crate::rpc::OutgoingSender;

#[tokio::test]
async fn an_ended_session_hands_the_editors_rust_requests_back_to_the_fallback() {
    let (inner, mut passed_through) = fake::session(|connection| {
        for message in &connection.receiver {
            if matches!(message, Message::Request(_)) {
                return;
            }
        }
    });
    let (tx, mut editor) = tokio::sync::mpsc::unbounded_channel();
    let backend = Backend::new(OutgoingSender::new(tx));
    *backend.analyzer.state.lock().unwrap() = AnalyzerState::Ready(Arc::new(Analyzer::from(inner)));

    assert!(
        backend.pass_request(json!(1), "textDocument/hover", json!({})),
        "a running session takes the request"
    );
    let unanswered = tokio::time::timeout(Duration::from_secs(10), passed_through.recv())
        .await
        .expect("the request was left waiting after the session ended")
        .unwrap();
    assert_eq!(unanswered["id"], json!(1));
    assert!(unanswered.get("error").is_some(), "{unanswered}");

    assert!(
        !backend.pass_request(json!(2), "textDocument/hover", json!({})),
        "an ended session takes no more, so the request is answered here instead"
    );
    assert!(backend.analyzer.ready().is_none());
    let told = editor
        .try_recv()
        .expect("the user is told rust-analyzer stopped");
    assert_eq!(told["method"], json!("window/showMessage"));
    assert!(
        editor.try_recv().is_err(),
        "the user is told once, not on every request"
    );
}
