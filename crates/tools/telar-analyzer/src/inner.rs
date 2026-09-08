//! The real rust-analyzer, run in this process over an in-memory LSP connection.
//!
//! Owning it here is what makes `[logic]` completion possible at all. The generated Rust reaches rust-analyzer as a `didChange` on the same ordered channel the query travels on, so a keystroke is analysed without the text ever touching disk — the cross-process bridge this replaces could not win that race, and answered hover and go-to-definition only.

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

/// How long the handshake may take before startup is called a failure. Generous: it only covers rust-analyzer answering `initialize`, which precedes any workspace loading.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

type Pending = Arc<std::sync::Mutex<HashMap<RequestId, oneshot::Sender<Value>>>>;

/// What rust-analyzer has been told a file contains: the document version it was sent under, and a digest of the text so an unchanged buffer is not resent.
struct Synced {
    version: i32,
    digest: u64,
}

pub struct Inner {
    to_ra: Sender<Message>,
    pending: Pending,
    next_id: AtomicI32,
    synced: std::sync::Mutex<HashMap<PathBuf, Synced>>,
}

impl Inner {
    /// Boots rust-analyzer against the cargo workspace at `root` and completes the LSP handshake. Blocking and slow enough to keep off the runtime thread; the workspace load it kicks off continues in the background, and queries before it finishes simply go unanswered.
    pub fn start(root: &Path) -> anyhow::Result<Self> {
        let (ra_side, our_side) = Connection::memory();
        let abs = AbsPathBuf::assert_utf8(root.to_path_buf());
        std::thread::Builder::new()
            .name("rust-analyzer".to_owned())
            .spawn(move || run_server(ra_side, abs))?;

        let Connection { sender, receiver } = our_side;
        handshake(&sender, &receiver, root)?;

        let pending: Pending = Arc::default();
        std::thread::Builder::new()
            .name("rust-analyzer-reader".to_owned())
            .spawn({
                let pending = pending.clone();
                let sender = sender.clone();
                move || read_loop(receiver, sender, pending)
            })?;

        Ok(Self {
            to_ra: sender,
            pending,
            next_id: AtomicI32::new(1),
            synced: std::sync::Mutex::default(),
        })
    }

    /// Sends a request and waits for its reply. Yields `None` when rust-analyzer never answers, which is what it does for analysis requests that arrive before the workspace has loaded.
    pub async fn request(&self, method: &str, params: Value, timeout: Duration) -> Option<Value> {
        let id = RequestId::from(self.next_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = oneshot::channel();
        self.pending.lock().ok()?.insert(id.clone(), tx);
        let sent = self.to_ra.send(Message::Request(Request {
            id: id.clone(),
            method: method.to_owned(),
            params,
        }));
        if sent.is_err() {
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
        config.rediscover_workspaces();
        let server_caps = rust_analyzer::server_capabilities(&config);
        connection.initialize_finish(id, json!({ "capabilities": server_caps }))?;
        rust_analyzer::main_loop(config, connection)
    })();
    if let Err(e) = session {
        eprintln!("telar-analyzer: rust-analyzer session ended: {e:#}");
    }
}

fn handshake(
    sender: &Sender<Message>,
    receiver: &Receiver<Message>,
    root: &Path,
) -> anyhow::Result<()> {
    let id = RequestId::from(0);
    sender.send(Message::Request(Request {
        id: id.clone(),
        method: "initialize".to_owned(),
        params: json!({
            "processId": std::process::id(),
            "rootUri": uri_for(root),
            "capabilities": client_capabilities(),
        }),
    }))?;

    let deadline = std::time::Instant::now() + HANDSHAKE_TIMEOUT;
    loop {
        let left = deadline
            .checked_duration_since(std::time::Instant::now())
            .ok_or_else(|| anyhow::anyhow!("rust-analyzer did not answer initialize"))?;
        match receiver.recv_timeout(left)? {
            Message::Response(Response { id: got, .. }) if got == id => break,
            Message::Request(request) => answer(sender, request),
            _ => {}
        }
    }
    sender.send(Message::Notification(Notification {
        method: "initialized".to_owned(),
        params: json!({}),
    }))?;
    Ok(())
}

/// What we ask rust-analyzer for. Deliberately narrow: every capability here is one the `.rsx` side maps back onto the source, and advertising more only grows what has to be translated. `definition` stays without `linkSupport` so navigation comes back as plain `Location`s.
fn client_capabilities() -> Value {
    json!({
        "textDocument": {
            "completion": { "completionItem": { "snippetSupport": false, "documentationFormat": ["markdown", "plaintext"] } },
            "hover": { "contentFormat": ["markdown", "plaintext"] },
            "signatureHelp": {},
            "definition": {},
            "references": {},
            "inlayHint": {},
            "diagnostic": {},
        },
    })
}

fn read_loop(receiver: Receiver<Message>, sender: Sender<Message>, pending: Pending) {
    for message in receiver {
        match message {
            Message::Response(Response { id, result, .. }) => {
                let waiting = pending.lock().ok().and_then(|mut p| p.remove(&id));
                if let Some(waiting) = waiting {
                    let _ = waiting.send(result.unwrap_or(Value::Null));
                }
            }
            Message::Request(request) => answer(&sender, request),
            Message::Notification(_) => {}
        }
    }
}

/// rust-analyzer blocks its own loading on `workspace/configuration`, so it has to be answered even though we have no settings to give it — an empty object per item leaves every setting at its default.
fn answer(sender: &Sender<Message>, Request { id, method, params }: Request) {
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
