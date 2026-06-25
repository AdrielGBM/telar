//! JSON-RPC framing over stdio plus the server→client message channel.
//!
//! Shared by the server loop (talking to the editor) and [`crate::ra_client`]
//! (talking to rust-analyzer), so both speak the same `Content-Length` framing.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use lsp_types::{Diagnostic, MessageType, PublishDiagnosticsParams, Registration, Uri};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::UnboundedSender;

/// Writes one `Content-Length`-framed JSON-RPC message.
pub async fn write_message<W>(writer: &mut W, msg: &Value) -> tokio::io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let body = serde_json::to_string(msg)
        .map_err(|e| tokio::io::Error::new(tokio::io::ErrorKind::InvalidData, e))?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(body.as_bytes()).await?;
    writer.flush().await
}

/// Reads one `Content-Length`-framed JSON-RPC message, returning an error when the stream closes.
pub async fn read_message<R>(reader: &mut BufReader<R>) -> tokio::io::Result<Value>
where
    R: AsyncRead + Unpin,
{
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            return Err(tokio::io::Error::new(
                tokio::io::ErrorKind::UnexpectedEof,
                "stream closed",
            ));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length: ") {
            content_length = value.parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map_err(|e| tokio::io::Error::new(tokio::io::ErrorKind::InvalidData, e))
}

/// Sends server→client messages (responses, notifications, requests) onto a
/// channel that the stdout writer task drains, so handlers never touch stdout
/// directly and writes never interleave.
#[derive(Clone)]
pub struct OutgoingSender {
    tx: UnboundedSender<Value>,
    next_id: Arc<AtomicI64>,
}

impl OutgoingSender {
    pub fn new(tx: UnboundedSender<Value>) -> Self {
        Self {
            tx,
            next_id: Arc::new(AtomicI64::new(1)),
        }
    }

    pub fn send(&self, msg: Value) {
        let _ = self.tx.send(msg);
    }

    pub fn publish_diagnostics(&self, uri: Uri, diagnostics: Vec<Diagnostic>) {
        let params = PublishDiagnosticsParams {
            uri,
            diagnostics,
            version: None,
        };
        self.notify(
            "textDocument/publishDiagnostics",
            serde_json::to_value(params).unwrap_or(Value::Null),
        );
    }

    pub fn log_message(&self, typ: MessageType, message: impl Into<String>) {
        self.notify(
            "window/logMessage",
            json!({ "type": typ, "message": message.into() }),
        );
    }

    /// Fires a `client/registerCapability` request; the client's response is ignored.
    pub fn register_capability(&self, registrations: Vec<Registration>) {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.send(json!({
            "jsonrpc": "2.0",
            "id": format!("rsx-reg-{id}"),
            "method": "client/registerCapability",
            "params": { "registrations": registrations },
        }));
    }

    fn notify(&self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }
}
