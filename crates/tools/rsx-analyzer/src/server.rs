//! Hand-rolled LSP server loop over stdio.
//!
//! Replaces the tower-lsp runtime: it frames JSON-RPC over stdin/stdout, routes
//! requests and notifications to [`Backend`] methods, and writes replies through
//! the [`OutgoingSender`] channel. State-mutating notifications (`didOpen` /
//! `didChange` / `didClose`) are awaited in order; requests are spawned so a slow
//! `rustfmt` run never stalls the read loop.

use std::sync::Arc;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::io::BufReader;
use tokio::sync::mpsc;

use crate::backend::Backend;
use crate::rpc::{OutgoingSender, read_message, write_message};

pub async fn run() {
    let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
    let backend = Arc::new(Backend::new(OutgoingSender::new(tx)));

    // A single writer task owns stdout so server-originated messages never interleave.
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(msg) = rx.recv().await {
            if write_message(&mut stdout, &msg).await.is_err() {
                break;
            }
        }
    });

    let mut stdin = BufReader::new(tokio::io::stdin());
    loop {
        let message = match read_message(&mut stdin).await {
            Ok(message) => message,
            Err(_) => break,
        };

        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // No `method`: this is a response to a server-initiated request — ignore it.
            continue;
        };
        if method == "exit" {
            break;
        }
        let method = method.to_string();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let id = message.get("id").cloned().filter(|id| !id.is_null());

        match id {
            Some(id) => {
                let backend = backend.clone();
                tokio::spawn(async move {
                    let response = match dispatch_request(&backend, &method, params).await {
                        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                        Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
                    };
                    backend.outgoing().send(response);
                });
            }
            None => dispatch_notification(&backend, &method, params).await,
        }
    }

    drop(backend);
    let _ = writer.await;
}

async fn dispatch_request(backend: &Backend, method: &str, params: Value) -> Result<Value, Value> {
    let result = match method {
        "initialize" => ok(backend.initialize()),
        "shutdown" => Value::Null,
        "textDocument/completion" => ok(backend.completion(parse(params)?).await),
        "textDocument/signatureHelp" => ok(backend.signature_help(parse(params)?).await),
        "textDocument/hover" => ok(backend.hover(parse(params)?).await),
        "textDocument/definition" => ok(backend.goto_definition(parse(params)?).await),
        "textDocument/formatting" => ok(backend.formatting(parse(params)?).await),
        "textDocument/documentColor" => ok(backend.document_color(parse(params)?).await),
        "textDocument/colorPresentation" => ok(backend.color_presentation(parse(params)?)),
        "textDocument/documentSymbol" => ok(backend.document_symbol(parse(params)?).await),
        "textDocument/foldingRange" => ok(backend.folding_range(parse(params)?).await),
        "textDocument/codeAction" => ok(backend.code_action(parse(params)?).await),
        "textDocument/codeLens" => ok(backend.code_lens(parse(params)?).await),
        "textDocument/documentHighlight" => ok(backend.document_highlight(parse(params)?).await),
        "textDocument/references" => ok(backend.references(parse(params)?).await),
        "textDocument/prepareRename" => ok(backend.prepare_rename(parse(params)?).await),
        "textDocument/rename" => ok(backend.rename(parse(params)?).await),
        "workspace/symbol" => ok(backend.workspace_symbol(parse(params)?).await),
        "textDocument/semanticTokens/full" => {
            ok(backend.semantic_tokens_full(parse(params)?).await)
        }
        _ => return Err(method_not_found(method)),
    };
    Ok(result)
}

async fn dispatch_notification(backend: &Backend, method: &str, params: Value) {
    match method {
        "initialized" => backend.initialized(),
        "textDocument/didOpen" => {
            if let Ok(params) = serde_json::from_value(params) {
                backend.did_open(params).await;
            }
        }
        "textDocument/didChange" => {
            if let Ok(params) = serde_json::from_value(params) {
                backend.did_change(params).await;
            }
        }
        "textDocument/didClose" => {
            if let Ok(params) = serde_json::from_value(params) {
                backend.did_close(params).await;
            }
        }
        _ => {}
    }
}

fn parse<T: DeserializeOwned>(params: Value) -> Result<T, Value> {
    serde_json::from_value(params).map_err(|err| invalid_params(&err.to_string()))
}

fn ok<T: Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn method_not_found(method: &str) -> Value {
    json!({ "code": -32601, "message": format!("method not found: {method}") })
}

fn invalid_params(message: &str) -> Value {
    json!({ "code": -32602, "message": message })
}
