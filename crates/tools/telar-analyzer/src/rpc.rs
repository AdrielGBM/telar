//! JSON-RPC framing over stdio plus the server→client message channel.
//!
//! Used by the server loop to talk to the editor with `Content-Length` framing.

use lsp_types::{Diagnostic, MessageType, PublishDiagnosticsParams, Uri};
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

/// The largest body a frame may announce; a corrupt length must not become a gigabyte allocation.
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

/// Why no message could be read. Every variant but [`FrameError::Closed`] leaves the stream at an unknown offset, so the session cannot go on.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("the stream closed")]
    Closed,
    #[error("a frame header has no Content-Length")]
    MissingContentLength,
    #[error("Content-Length `{0}` is not a number")]
    InvalidContentLength(String),
    #[error("Content-Length {length} exceeds the {MAX_BODY_BYTES} byte limit")]
    TooLarge { length: usize },
    #[error("the frame body is not JSON: {0}")]
    InvalidBody(#[source] serde_json::Error),
    #[error("reading the stream failed: {0}")]
    Io(#[from] tokio::io::Error),
}

/// Reads one `Content-Length`-framed JSON-RPC message; [`FrameError::Closed`] when the stream ends between frames.
pub async fn read_message<R>(reader: &mut BufReader<R>) -> Result<Value, FrameError>
where
    R: AsyncRead + Unpin,
{
    let mut content_length = None;
    let mut first_line = true;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Err(match first_line {
                true => FrameError::Closed,
                false => tokio::io::Error::from(tokio::io::ErrorKind::UnexpectedEof).into(),
            });
        }
        first_line = false;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length: ") {
            let length = value
                .parse()
                .map_err(|_| FrameError::InvalidContentLength(value.to_owned()))?;
            content_length = Some(length);
        }
    }
    let length = content_length.ok_or(FrameError::MissingContentLength)?;
    if length > MAX_BODY_BYTES {
        return Err(FrameError::TooLarge { length });
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).await?;
    serde_json::from_slice(&body).map_err(FrameError::InvalidBody)
}

/// The JSON-RPC error for a well-formed request that could not be carried out, with the reason the editor shows.
pub fn request_failed(message: &str) -> Value {
    json!({ "code": lsp_types::error_codes::REQUEST_FAILED, "message": message })
}

/// Sends server→client messages (responses, notifications, requests) onto a channel that the stdout writer task drains, so handlers never touch stdout directly and writes never interleave.
#[derive(Clone)]
pub struct OutgoingSender {
    tx: UnboundedSender<Value>,
}

impl OutgoingSender {
    pub fn new(tx: UnboundedSender<Value>) -> Self {
        Self { tx }
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

    /// A message the editor puts in front of the user rather than only in the log. Reserved for what they have to act on, such as a server that came up degraded.
    pub fn show_message(&self, typ: MessageType, message: impl Into<String>) {
        self.notify(
            "window/showMessage",
            json!({ "type": typ, "message": message.into() }),
        );
    }

    fn notify(&self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }
}

#[cfg(test)]
#[path = "rpc.test.rs"]
mod tests;
