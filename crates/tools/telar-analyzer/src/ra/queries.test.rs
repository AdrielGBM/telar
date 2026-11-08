use super::*;

use lsp_server::{ErrorCode, Response};

use crate::inner::fake;

const GENERATED: &str = "pub fn feature_card() {}\n";

fn gen_path() -> PathBuf {
    std::env::temp_dir().join("telar-queries/.telar/build/feature_card.rs")
}

fn analyzer_answering(
    answer: impl Fn(lsp_server::Request) -> Response + Send + 'static,
) -> Analyzer {
    let (inner, _editor) = fake::session(fake::answering(answer));
    Analyzer::from(inner)
}

#[tokio::test]
async fn references_keep_the_error_rust_analyzer_answered_with() {
    let analyzer = analyzer_answering(|request| {
        Response::new_err(
            request.id,
            ErrorCode::RequestFailed as i32,
            "no crate graph".to_owned(),
        )
    });

    let refs = analyzer.references(&gen_path(), GENERATED, 7).await;

    match refs {
        Err(ReferencesError::Request(RequestError::Answered(error))) => {
            assert_eq!(error.message, "no crate graph");
        }
        other => panic!(
            "expected rust-analyzer's error, got {:?}",
            other.map(|refs| refs.len())
        ),
    }
}

/// The declaration is always asked for, so a null answer cannot mean a symbol nobody uses: reading it as "no references" is what let a rename leave Rust naming the old name.
#[tokio::test]
async fn references_answered_with_null_are_an_error_rather_than_none() {
    let analyzer = analyzer_answering(|request| Response::new_ok(request.id, Value::Null));

    let refs = analyzer.references(&gen_path(), GENERATED, 7).await;

    assert!(
        matches!(refs, Err(ReferencesError::NoSymbol)),
        "expected no symbol, got {:?}",
        refs.map(|refs| refs.len())
    );
}

#[tokio::test]
async fn references_rust_analyzer_found_are_returned_with_their_files() {
    let path = gen_path();
    let uri = crate::uri::from_path(&path).unwrap();
    let analyzer = analyzer_answering(move |request| {
        Response::new_ok(
            request.id,
            json!([{
                "uri": uri.as_str(),
                "range": { "start": { "line": 0, "character": 7 }, "end": { "line": 0, "character": 19 } },
            }]),
        )
    });

    let refs = analyzer
        .references(&path, GENERATED, 7)
        .await
        .expect("rust-analyzer named the declaration");

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].path, path);
    assert_eq!(refs[0].range.start, Position::new(0, 7));
}
