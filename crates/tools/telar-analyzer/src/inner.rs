//! The real rust-analyzer, run in this process over an in-memory LSP connection.
//!
//! It serves two masters. The `.rsx` side queries it directly (see [`crate::ra`]): the generated Rust reaches it as a `didChange` on the same ordered channel the query travels on, so a keystroke is analysed without the text ever touching disk — the cross-process bridge this replaces could not win that race, and answered hover and go-to-definition only. Everything the editor asks about real `.rs` files is passed through instead, which is what lets one rust-analyzer serve both and leaves the editor with a single index.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use paths::AbsPathBuf;
use rust_analyzer::config::Config;
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::rpc::OutgoingSender;

/// How long the handshake may take before startup is called a failure. Generous: it only covers rust-analyzer answering `initialize`, which precedes any workspace loading.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

enum Waiting {
    /// A query the `.rsx` side asked; the answer goes back to the awaiting task.
    Ours(oneshot::Sender<Value>),
    /// A request the editor asked and we passed through; the answer goes back to the editor under the id it used.
    Editor(Value),
}

type Pending = Arc<std::sync::Mutex<HashMap<RequestId, Waiting>>>;
/// Requests rust-analyzer sent to the client, relayed onward under an id of ours: maps that id back to the one rust-analyzer is waiting on.
type Relayed = Arc<std::sync::Mutex<HashMap<i32, RequestId>>>;

/// What rust-analyzer has been told a file contains: the document version it was sent under, and a digest of the text so an unchanged buffer is not resent.
struct Synced {
    version: i32,
    digest: u64,
}

pub struct Inner {
    to_ra: Sender<Message>,
    pending: Pending,
    relayed: Relayed,
    next_id: Arc<AtomicI32>,
    synced: std::sync::Mutex<HashMap<PathBuf, Synced>>,
    /// rust-analyzer's own `initialize` result, so the editor is offered what it can do for `.rs` alongside what we do for `.rsx`.
    capabilities: Value,
}

impl Inner {
    /// Boots rust-analyzer against the cargo workspace at `root` and completes the LSP handshake, passing the editor's own `initializationOptions` through so the user's `rust-analyzer.*` settings are the ones that apply. Blocking and slow enough to keep off the runtime thread; the workspace load it kicks off continues in the background.
    pub fn start(root: &Path, options: Value, outgoing: OutgoingSender) -> anyhow::Result<Self> {
        let (ra_side, our_side) = Connection::memory();
        let abs = AbsPathBuf::assert_utf8(root.to_path_buf());
        std::thread::Builder::new()
            .name("rust-analyzer".to_owned())
            .spawn(move || run_server(ra_side, abs))?;

        let Connection { sender, receiver } = our_side;
        let capabilities = handshake(&sender, &receiver, root, options)?;

        let pending: Pending = Arc::default();
        let relayed: Relayed = Arc::default();
        let next_id = Arc::new(AtomicI32::new(1));
        std::thread::Builder::new()
            .name("rust-analyzer-reader".to_owned())
            .spawn({
                let pending = pending.clone();
                let relayed = relayed.clone();
                let next_id = next_id.clone();
                move || read_loop(receiver, pending, relayed, next_id, outgoing)
            })?;

        Ok(Self {
            to_ra: sender,
            pending,
            relayed,
            next_id,
            synced: std::sync::Mutex::default(),
            capabilities,
        })
    }

    /// What rust-analyzer told us it can do, for merging into our own `initialize` result.
    pub fn capabilities(&self) -> &Value {
        &self.capabilities
    }

    /// Sends a request and waits for its reply. Yields `None` when rust-analyzer never answers, which is what it does for analysis requests that arrive before the workspace has loaded.
    pub async fn request(&self, method: &str, params: Value, timeout: Duration) -> Option<Value> {
        let id = self.claim_id();
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .ok()?
            .insert(id.clone(), Waiting::Ours(tx));
        if !self.send_request(&id, method, params) {
            self.pending.lock().ok()?.remove(&id);
            return None;
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(value)) => Some(value),
            // Timed out, or the server dropped the reply: drop the slot so it cannot leak for the life of the session.
            _ => {
                self.pending.lock().ok()?.remove(&id);
                None
            }
        }
    }

    pub fn notify(&self, method: &str, params: Value) {
        let _ = self.to_ra.send(Message::Notification(Notification {
            method: method.to_owned(),
            params,
        }));
    }

    /// Passes an editor request through to rust-analyzer under an id of ours, so its reply can be routed back under the editor's. Nothing is awaited here — the reply travels the read loop like any other.
    pub fn pass_request(&self, editor_id: Value, method: &str, params: Value) {
        let id = self.claim_id();
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(id.clone(), Waiting::Editor(editor_id));
        }
        let _ = self.send_request(&id, method, params);
    }

    /// Routes the editor's answer back to the rust-analyzer request that is waiting on it. A reply for an id we never relayed is dropped.
    pub fn relay_reply(&self, our_id: i32, result: Option<Value>, error: Option<Value>) {
        let Some(ra_id) = self.relayed.lock().ok().and_then(|mut r| r.remove(&our_id)) else {
            return;
        };
        let _ = self.to_ra.send(Message::Response(Response {
            id: ra_id,
            result: Some(result.unwrap_or(Value::Null)),
            error: error.and_then(|e| serde_json::from_value(e).ok()),
        }));
    }

    /// Puts `text` in front of rust-analyzer as the content of `path`, opening the document on first use. Text it already holds is not resent: every query syncs before asking, and a `didChange` invalidates salsa whether or not anything changed.
    pub fn sync(&self, path: &Path, text: &str) {
        let digest = digest(text);
        let Ok(mut synced) = self.synced.lock() else {
            return;
        };
        let uri = uri_for(path);
        match synced.get_mut(path) {
            Some(state) if state.digest == digest => {}
            Some(state) => {
                state.version += 1;
                state.digest = digest;
                let version = state.version;
                drop(synced);
                self.notify(
                    "textDocument/didChange",
                    json!({
                        "textDocument": { "uri": uri, "version": version },
                        "contentChanges": [{ "text": text }],
                    }),
                );
            }
            None => {
                synced.insert(path.to_path_buf(), Synced { version: 1, digest });
                drop(synced);
                self.notify(
                    "textDocument/didOpen",
                    json!({
                        "textDocument": { "uri": uri, "languageId": "rust", "version": 1, "text": text },
                    }),
                );
            }
        }
    }

    fn claim_id(&self) -> RequestId {
        RequestId::from(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Whether the request reached rust-analyzer. A closed channel means the session is gone, which every caller treats as "no answer" rather than an error worth surfacing.
    fn send_request(&self, id: &RequestId, method: &str, params: Value) -> bool {
        self.to_ra
            .send(Message::Request(Request {
                id: id.clone(),
                method: method.to_owned(),
                params,
            }))
            .is_ok()
    }
}

/// The `file://` form rust-analyzer expects. Percent-encoding matters: a path holding a space or a non-ASCII segment has to produce the same URI the editor itself would send, or the two disagree about which file is open.
pub fn uri_for(path: &Path) -> String {
    crate::uri::from_path(path)
        .map(|uri| uri.as_str().to_owned())
        .unwrap_or_else(|| format!("file://{}", path.display()))
}

fn digest(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Mirrors what `rust_analyzer::session::run_session` does for stdio, which cannot be reused: its `IoThreads` has no variant for an in-memory connection.
fn run_server(connection: Connection, root: AbsPathBuf) {
    let session = (|| -> anyhow::Result<()> {
        let (id, params) = connection.initialize_start()?;
        let caps = serde_json::from_value(params.get("capabilities").cloned().unwrap_or(json!({})))
            .unwrap_or_default();
        let mut config = Config::new(root.clone(), caps, vec![root], None);
        if let Some(options) = params.get("initializationOptions").filter(|o| !o.is_null()) {
            config = apply_client_options(config, options.clone());
        }
        config.rediscover_workspaces();
        let server_caps = rust_analyzer::server_capabilities(&config);
        connection.initialize_finish(id, json!({ "capabilities": server_caps }))?;
        rust_analyzer::main_loop(config, connection)
    })();
    if let Err(e) = session {
        eprintln!("telar-analyzer: rust-analyzer session ended: {e:#}");
    }
}

/// Folds the editor's `rust-analyzer.*` settings into the config. Errors are ignored rather than fatal: a setting this pinned snapshot does not know must not stop the server the user is waiting on.
fn apply_client_options(config: Config, options: Value) -> Config {
    let mut change = rust_analyzer::config::ConfigChange::default();
    change.change_client_config(options);
    config.apply_change(change).0
}

/// Runs the editor's `initialize` against rust-analyzer, answering the requests it makes before we have a reply to relay them into. Returns its advertised capabilities.
fn handshake(
    sender: &Sender<Message>,
    receiver: &Receiver<Message>,
    root: &Path,
    options: Value,
) -> anyhow::Result<Value> {
    let id = RequestId::from(0);
    sender.send(Message::Request(Request {
        id: id.clone(),
        method: "initialize".to_owned(),
        params: json!({
            "processId": std::process::id(),
            "rootUri": uri_for(root),
            "capabilities": client_capabilities(),
            "initializationOptions": options,
        }),
    }))?;

    let deadline = std::time::Instant::now() + HANDSHAKE_TIMEOUT;
    let capabilities = loop {
        let left = deadline
            .checked_duration_since(std::time::Instant::now())
            .ok_or_else(|| anyhow::anyhow!("rust-analyzer did not answer initialize"))?;
        match receiver.recv_timeout(left)? {
            Message::Response(Response {
                id: got, result, ..
            }) if got == id => {
                break result
                    .and_then(|r| r.get("capabilities").cloned())
                    .unwrap_or(json!({}));
            }
            Message::Request(request) => answer_locally(sender, request),
            _ => {}
        }
    };
    sender.send(Message::Notification(Notification {
        method: "initialized".to_owned(),
        params: json!({}),
    }))?;
    Ok(capabilities)
}

/// What we tell rust-analyzer the client can do. It is the union of what the `.rsx` side maps back onto the source and what we pass straight through, so `.rs` files keep the features the editor would have had talking to rust-analyzer directly.
fn client_capabilities() -> Value {
    json!({
        "workspace": {
            "configuration": true,
            "didChangeWatchedFiles": { "dynamicRegistration": true },
            "workspaceFolders": true,
            "applyEdit": true,
            "workspaceEdit": { "documentChanges": true },
            "symbol": {},
        },
        "textDocument": {
            "synchronization": { "didSave": true },
            "completion": { "completionItem": { "snippetSupport": false, "documentationFormat": ["markdown", "plaintext"] } },
            "hover": { "contentFormat": ["markdown", "plaintext"] },
            "signatureHelp": {},
            "definition": {},
            "typeDefinition": {},
            "implementation": {},
            "declaration": {},
            "references": {},
            "documentHighlight": {},
            "documentSymbol": {},
            "codeAction": { "codeActionLiteralSupport": { "codeActionKind": { "valueSet": [] } } },
            "codeLens": {},
            "formatting": {},
            "rangeFormatting": {},
            "rename": { "prepareSupport": true },
            "foldingRange": {},
            "selectionRange": {},
            "semanticTokens": {},
            "inlayHint": {},
            "callHierarchy": {},
            "diagnostic": {},
        },
        "window": { "workDoneProgress": true },
        "experimental": {},
    })
}

fn read_loop(
    receiver: Receiver<Message>,
    pending: Pending,
    relayed: Relayed,
    next_id: Arc<AtomicI32>,
    outgoing: OutgoingSender,
) {
    for message in receiver {
        match message {
            Message::Response(Response { id, result, error }) => {
                let waiting = pending.lock().ok().and_then(|mut p| p.remove(&id));
                match waiting {
                    Some(Waiting::Ours(tx)) => {
                        let _ = tx.send(result.unwrap_or(Value::Null));
                    }
                    Some(Waiting::Editor(editor_id)) => {
                        outgoing.send(match error {
                            Some(error) => json!({ "jsonrpc": "2.0", "id": editor_id, "error": error }),
                            None => json!({ "jsonrpc": "2.0", "id": editor_id, "result": result.unwrap_or(Value::Null) }),
                        });
                    }
                    None => {}
                }
            }
            Message::Request(Request { id, method, params }) => {
                let ours = next_id.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut relayed) = relayed.lock() {
                    relayed.insert(ours, id);
                }
                outgoing.send(
                    json!({ "jsonrpc": "2.0", "id": ours, "method": method, "params": params }),
                );
            }
            Message::Notification(Notification { method, params }) => {
                // Diagnostics for the generated file travel on too: the editor-side bridge projects the cargo-check errors among them back onto the `.rsx`, which the pull path does not cover.
                outgoing.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
            }
        }
    }
}

/// rust-analyzer blocks its own loading on `workspace/configuration`, and during the handshake there is no editor reply to relay yet — we have not answered its `initialize`, so it is not listening. An empty object per item leaves every setting at the default the options already applied.
fn answer_locally(sender: &Sender<Message>, Request { id, method, params }: Request) {
    let result = if method == "workspace/configuration" {
        let items = params
            .get("items")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        Value::Array(vec![json!({}); items])
    } else {
        Value::Null
    };
    let _ = sender.send(Message::Response(Response {
        id,
        result: Some(result),
        error: None,
    }));
}

#[cfg(test)]
#[path = "inner_test.rs"]
mod tests;
