//! The system clipboard, through the browser's asynchronous one.

use std::cell::RefCell;

use services_core::Clipboard;
use wasm_bindgen_futures::JsFuture;

// What the last read came back with. A thread-local rather than a field, so the clipboard itself is a unit
// struct and satisfies the `Send + Sync` the service is declared with — which on a target with one thread
// costs nothing and means nothing, but has to be true for the type to be installed at all.
thread_local! {
    static CACHED: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Reads and writes the clipboard through `navigator.clipboard`.
///
/// That API is asynchronous and Telar's is not, which is a real gap rather than a detail to paper over: a
/// read is started when it is asked for and its answer is available to the *next* read. In practice the
/// widget that pastes has already been handed what the browser gave the page — a paste gesture delivers its
/// text through the event, not through a poll — and this covers the rest: what the app itself copied reads
/// back immediately, and a foreign copy arrives one gesture late rather than never.
#[derive(Clone, Copy, Default, Debug)]
pub struct WebClipboard;

impl WebClipboard {
    pub fn new() -> Self {
        Self
    }
}

fn clipboard() -> web_sys::Clipboard {
    crate::dom::window().navigator().clipboard()
}

fn start_read() {
    let promise = clipboard().read_text();
    wasm_bindgen_futures::spawn_local(async move {
        if let Ok(value) = JsFuture::from(promise).await
            && let Some(text) = value.as_string()
        {
            CACHED.with(|cached| *cached.borrow_mut() = Some(text));
        }
    });
}

impl Clipboard for WebClipboard {
    fn text(&self) -> Option<String> {
        // Start the next read before answering with the last: the answer is always one gesture behind, and
        // not starting one would leave it behind forever.
        start_read();
        CACHED.with(|cached| cached.borrow().clone())
    }

    fn set_text(&self, text: &str) {
        CACHED.with(|cached| *cached.borrow_mut() = Some(text.to_string()));
        let promise = clipboard().write_text(text);
        wasm_bindgen_futures::spawn_local(async move {
            let _ = JsFuture::from(promise).await;
        });
    }
}
