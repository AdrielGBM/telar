use lsp_server::{Connection, Message, Request, Response};
use serde_json::Value;
use tokio::sync::mpsc::UnboundedReceiver;

use super::Inner;
use crate::rpc::OutgoingSender;

/// An [`Inner`] whose rust-analyzer is `serve`, run on a thread of its own over the far end of the connection, and the channel the editor reads.
pub(crate) fn session(
    serve: impl FnOnce(Connection) + Send + 'static,
) -> (Inner, UnboundedReceiver<Value>) {
    let (theirs, ours) = Connection::memory();
    std::thread::spawn(move || serve(theirs));
    let (tx, editor) = tokio::sync::mpsc::unbounded_channel();
    let inner = Inner::connect(ours, Value::Null, OutgoingSender::new(tx))
        .expect("the reader thread could not be spawned");
    (inner, editor)
}

/// A rust-analyzer that answers every request with what `answer` makes of it, until the connection closes.
pub(crate) fn answering(
    answer: impl Fn(Request) -> Response + Send + 'static,
) -> impl FnOnce(Connection) + Send + 'static {
    move |connection| {
        for message in &connection.receiver {
            if let Message::Request(request) = message
                && connection
                    .sender
                    .send(Message::Response(answer(request)))
                    .is_err()
            {
                return;
            }
        }
    }
}
