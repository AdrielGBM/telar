//! Hand-rolled LSP server loop over stdio.
//!
//! Replaces the tower-lsp runtime: it frames JSON-RPC over stdin/stdout, routes requests and notifications to [`Backend`] methods, and writes replies through the [`OutgoingSender`] channel. State-mutating notifications (`didOpen` / `didChange` / `didClose`) are awaited in order; requests are spawned so a slow `rustfmt` run never stalls the read loop.

use std::sync::{Arc, Weak};
use std::time::Duration;

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

    spawn_shutdown_signals(Arc::downgrade(&backend));

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
        if method == "initialize" {
            watch_client_process(&params, Arc::downgrade(&backend));
        }

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

    backend.release_analyzer();
    drop(backend);
    // The writer ends only once the last `Arc<Backend>` drops and closes the outgoing channel, so a request handler still wedged on the analyzer mutex would otherwise strand the process here with the client already gone.
    let _ = tokio::time::timeout(SHUTDOWN_GRACE, writer).await;
}

const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);

/// Long, because this only backstops the client deaths that stdin EOF already covers.
#[cfg(unix)]
const CLIENT_POLL: Duration = Duration::from_secs(10);

/// Releases the analyzer and ends the process, for the paths that cannot unwind through [`run`].
#[cfg(unix)]
async fn exit_now(backend: &Weak<Backend>) -> ! {
    // Off the runtime thread: releasing waits on the analyzer mutex, which a blocking query may hold.
    if let Some(backend) = backend.upgrade() {
        let _ = tokio::task::spawn_blocking(move || backend.release_analyzer()).await;
    }
    std::process::exit(0);
}

/// Releases the analyzer before the process dies on SIGTERM/SIGHUP, which is how an editor usually terminates us when it does not get to send `exit`. SIGKILL stays out of reach: the child is spawned inside `ra_ap_proc_macro_api`, so `PR_SET_PDEATHSIG` is not ours to set without hand-rolling that spawn against the pinned `ra_ap_*` snapshot.
// Holds a `Weak`, never an `Arc`: this task outlives the read loop by design, so a strong handle would keep `Backend` — and with it the outgoing channel's only sender — alive past the `drop` in [`run`], leaving the writer awaiting a channel that can never close and the process alive forever.
#[cfg(unix)]
fn spawn_shutdown_signals(backend: Weak<Backend>) {
    use tokio::signal::unix::{SignalKind, signal};

    tokio::spawn(async move {
        let (Ok(mut term), Ok(mut hup)) = (
            signal(SignalKind::terminate()),
            signal(SignalKind::hangup()),
        ) else {
            return;
        };
        tokio::select! {
            _ = term.recv() => {}
            _ = hup.recv() => {}
        }
        exit_now(&backend).await;
    });
}

#[cfg(not(unix))]
fn spawn_shutdown_signals(_backend: Weak<Backend>) {}

/// Exits when the client vanishes without closing stdin. `initialize` carries `processId` so a server can detect exactly this; stdio normally delivers EOF first, so this only backstops a client that leaked the pipe's write end to a surviving child.
#[cfg(unix)]
fn watch_client_process(params: &Value, backend: Weak<Backend>) {
    let Some(pid) = params.get("processId").and_then(Value::as_i64) else {
        return;
    };
    let Ok(pid) = i32::try_from(pid) else { return };

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(CLIENT_POLL).await;
            // Signal 0 runs the existence and permission checks without delivering anything, and only ESRCH proves the pid is gone rather than merely unreachable.
            if unsafe { libc::kill(pid, 0) } == 0 {
                continue;
            }
            if std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
                continue;
            }
            exit_now(&backend).await;
        }
    });
}

#[cfg(not(unix))]
fn watch_client_process(_params: &Value, _backend: Weak<Backend>) {}

async fn dispatch_request(backend: &Backend, method: &str, params: Value) -> Result<Value, Value> {
    let result = match method {
        "initialize" => ok(backend.initialize()),
        "shutdown" => Value::Null,
        "textDocument/completion" => ok(backend.completion(parse(params)?).await),
        "completionItem/resolve" => ok(backend.completion_resolve(parse(params)?)),
        "textDocument/signatureHelp" => ok(backend.signature_help(parse(params)?).await),
        "textDocument/hover" => ok(backend.hover(parse(params)?).await),
        "textDocument/definition" => ok(backend.goto_definition(parse(params)?).await),
        "textDocument/formatting" => ok(backend.formatting(parse(params)?).await),
        "textDocument/rangeFormatting" => ok(backend.range_formatting(parse(params)?).await),
        "textDocument/selectionRange" => ok(backend.selection_range(parse(params)?).await),
        "textDocument/documentColor" => ok(backend.document_color(parse(params)?).await),
        "textDocument/colorPresentation" => ok(backend.color_presentation(parse(params)?)),
        "textDocument/documentSymbol" => ok(backend.document_symbol(parse(params)?).await),
        "textDocument/foldingRange" => ok(backend.folding_range(parse(params)?).await),
        "textDocument/codeAction" => ok(backend.code_action(parse(params)?).await),
        "textDocument/codeLens" => ok(backend.code_lens(parse(params)?).await),
        "textDocument/documentLink" => ok(backend.document_link(parse(params)?).await),
        "textDocument/inlayHint" => ok(backend.inlay_hint(parse(params)?).await),
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
        "workspace/didChangeWatchedFiles" => {
            if let Ok(params) = serde_json::from_value(params) {
                backend.did_change_watched_files(params).await;
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
