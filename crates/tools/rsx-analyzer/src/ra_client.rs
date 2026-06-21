use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, oneshot};
use tower_lsp::lsp_types::*;

pub struct RaClient {
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    next_id: Arc<AtomicU64>,
    _child: Arc<Mutex<Child>>,
    legend: tower_lsp::lsp_types::SemanticTokensLegend,
}

impl RaClient {
    pub async fn spawn(workspace_root: &Path) -> Option<Self> {
        let ra_path = find_rust_analyzer()?;
        let mut child = Command::new(&ra_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;

        let stdin = Arc::new(Mutex::new(child.stdin.take()?));
        let stdout = child.stdout.take()?;
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = pending.clone();

        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_lsp_message(&mut reader).await {
                    Ok(msg) => {
                        if let Some(id) = msg.get("id").and_then(|v| v.as_u64()) {
                            if let Some(tx) = pending_clone.lock().await.remove(&id) {
                                let _ = tx.send(msg);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let next_id = Arc::new(AtomicU64::new(1));
        let child_arc = Arc::new(Mutex::new(child));

        // Build a temporary client to perform the initialize handshake
        let tmp = Self {
            stdin: stdin.clone(),
            pending: pending.clone(),
            next_id: next_id.clone(),
            _child: child_arc.clone(),
            legend: tower_lsp::lsp_types::SemanticTokensLegend {
                token_types: vec![],
                token_modifiers: vec![],
            },
        };

        let workspace_uri = Url::from_file_path(workspace_root).ok()?;
        let init_resp = tmp
            .send_request(
                "initialize",
                json!({
                    "processId": std::process::id(),
                    "rootUri": workspace_uri,
                    "workspaceFolders": [{ "uri": workspace_uri.as_str(), "name": "rsx-lsp" }],
                    "capabilities": {
                        "textDocument": {
                            "completion": {
                                "completionItem": { "snippetSupport": false }
                            },
                            "hover": { "contentFormat": ["plaintext"] },
                            "definition": {},
                            "semanticTokens": {
                                "requests": { "full": true },
                                "formats": ["relative"]
                            }
                        }
                    }
                }),
            )
            .await;
        let legend = init_resp
            .as_ref()
            .and_then(|r| r.get("result"))
            .and_then(|r| r.get("capabilities"))
            .and_then(|c| c.get("semanticTokensProvider"))
            .and_then(|p| p.get("legend"))
            .cloned()
            .and_then(|v| {
                serde_json::from_value::<tower_lsp::lsp_types::SemanticTokensLegend>(v).ok()
            })
            .unwrap_or_else(|| tower_lsp::lsp_types::SemanticTokensLegend {
                token_types: vec![],
                token_modifiers: vec![],
            });

        let client = Self {
            stdin,
            pending,
            next_id,
            _child: child_arc,
            legend,
        };

        client.send_notification("initialized", json!({})).await;

        Some(client)
    }

    async fn send_request(&self, method: &str, params: Value) -> Option<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        write_lsp_message(&mut *self.stdin.lock().await, &msg)
            .await
            .ok()?;
        tokio::time::timeout(std::time::Duration::from_secs(10), rx)
            .await
            .ok()?
            .ok()
    }

    async fn send_notification(&self, method: &str, params: Value) {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        let _ = write_lsp_message(&mut *self.stdin.lock().await, &msg).await;
    }

    pub async fn did_open(&self, uri: &Url, text: &str) {
        self.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri.as_str(),
                    "languageId": "rust",
                    "version": 1,
                    "text": text
                }
            }),
        )
        .await;
    }

    pub async fn did_change(&self, uri: &Url, text: &str, version: i32) {
        self.send_notification(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri.as_str(), "version": version },
                "contentChanges": [{ "text": text }]
            }),
        )
        .await;
    }

    pub async fn did_close(&self, uri: &Url) {
        self.send_notification(
            "textDocument/didClose",
            json!({
                "textDocument": { "uri": uri.as_str() }
            }),
        )
        .await;
    }

    pub async fn completion(&self, uri: &Url, line: u32, character: u32) -> Vec<CompletionItem> {
        let result = self
            .send_request(
                "textDocument/completion",
                json!({
                    "textDocument": { "uri": uri.as_str() },
                    "position": { "line": line, "character": character }
                }),
            )
            .await;
        parse_completion_response(result)
    }

    pub async fn hover(&self, uri: &Url, line: u32, character: u32) -> Option<Hover> {
        let result = self
            .send_request(
                "textDocument/hover",
                json!({
                    "textDocument": { "uri": uri.as_str() },
                    "position": { "line": line, "character": character }
                }),
            )
            .await?;
        parse_hover_response(result)
    }

    pub async fn definition(
        &self,
        uri: &Url,
        line: u32,
        character: u32,
    ) -> Option<GotoDefinitionResponse> {
        let result = self
            .send_request(
                "textDocument/definition",
                json!({
                    "textDocument": { "uri": uri.as_str() },
                    "position": { "line": line, "character": character }
                }),
            )
            .await?;
        parse_definition_response(result)
    }

    pub fn legend(&self) -> &tower_lsp::lsp_types::SemanticTokensLegend {
        &self.legend
    }

    pub async fn semantic_tokens_full(&self, uri: &Url) -> Option<Vec<u32>> {
        let result = self
            .send_request(
                "textDocument/semanticTokens/full",
                json!({
                    "textDocument": { "uri": uri.as_str() }
                }),
            )
            .await?;
        let data = result.get("result")?.get("data")?;
        serde_json::from_value::<Vec<u32>>(data.clone()).ok()
    }
}

async fn write_lsp_message(stdin: &mut ChildStdin, msg: &Value) -> tokio::io::Result<()> {
    let body = serde_json::to_string(msg)
        .map_err(|e| tokio::io::Error::new(tokio::io::ErrorKind::InvalidData, e))?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    stdin.write_all(header.as_bytes()).await?;
    stdin.write_all(body.as_bytes()).await?;
    stdin.flush().await
}

async fn read_lsp_message<R>(reader: &mut BufReader<R>) -> tokio::io::Result<Value>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(tokio::io::Error::new(
                tokio::io::ErrorKind::UnexpectedEof,
                "rust-analyzer closed",
            ));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(v) = trimmed.strip_prefix("Content-Length: ") {
            content_length = v.parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map_err(|e| tokio::io::Error::new(tokio::io::ErrorKind::InvalidData, e))
}

fn find_rust_analyzer() -> Option<PathBuf> {
    let path_env = std::env::var("PATH").unwrap_or_default();
    let sep = if cfg!(windows) { ';' } else { ':' };
    let ext = if cfg!(windows) { ".exe" } else { "" };
    for dir in path_env.split(sep) {
        let p = Path::new(dir).join(format!("rust-analyzer{ext}"));
        if p.exists() {
            return Some(p);
        }
    }
    // Fall back to ~/.cargo/bin
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    let cargo_bin = Path::new(&home)
        .join(".cargo")
        .join("bin")
        .join(format!("rust-analyzer{ext}"));
    if cargo_bin.exists() {
        Some(cargo_bin)
    } else {
        None
    }
}

fn parse_completion_response(response: Option<Value>) -> Vec<CompletionItem> {
    let Some(result) = response.as_ref().and_then(|r| r.get("result")) else {
        return vec![];
    };
    // result is either CompletionList { items: [...] } or [...] directly
    let items = if let Some(arr) = result.as_array() {
        arr.clone()
    } else if let Some(arr) = result.get("items").and_then(|v| v.as_array()) {
        arr.clone()
    } else {
        return vec![];
    };
    items
        .into_iter()
        .filter_map(|item| serde_json::from_value::<CompletionItem>(item).ok())
        .collect()
}

fn parse_hover_response(response: Value) -> Option<Hover> {
    let result = response.get("result")?;
    if result.is_null() {
        return None;
    }
    serde_json::from_value::<Hover>(result.clone()).ok()
}

fn parse_definition_response(response: Value) -> Option<GotoDefinitionResponse> {
    let result = response.get("result")?;
    if result.is_null() {
        return None;
    }
    // Try as array of locations first
    if let Ok(locs) = serde_json::from_value::<Vec<Location>>(result.clone()) {
        if locs.is_empty() {
            return None;
        }
        return Some(GotoDefinitionResponse::Array(locs));
    }
    // Try as single location
    if let Ok(loc) = serde_json::from_value::<Location>(result.clone()) {
        return Some(GotoDefinitionResponse::Scalar(loc));
    }
    None
}
