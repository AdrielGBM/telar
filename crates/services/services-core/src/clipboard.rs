//! The system clipboard, as the vocabulary crate sees it.

use std::sync::{Arc, OnceLock};

/// Reading and writing the system selection.
///
/// `Send + Sync` for the same reason [`FileDialogs`](crate::dialogs::FileDialogs) is: a backend may have to serve the bytes from a thread of its own. Wayland in particular hands the compositor a *source* rather than a copy, and then serves whoever pastes — possibly minutes later — so a backend that owns a selection owns a thread with it.
///
/// Both methods can fail quietly: a headless session has no clipboard, and a compositor can refuse. A caller that pastes gets `None` and leaves the text alone, which is what every editor does when the selection is empty anyway.
pub trait Clipboard: Send + Sync + 'static {
    /// The text currently on the clipboard, or `None` when it holds nothing this can read.
    fn text(&self) -> Option<String>;
    /// Puts `text` on the clipboard, replacing whatever was there.
    fn set_text(&self, text: &str);
}

static CLIPBOARD: OnceLock<Arc<dyn Clipboard>> = OnceLock::new();

/// Installs the backend the app's clipboard goes through. The desktop runner calls this at startup; a shell that speaks the protocol itself installs its own, and a test can install a stub. The first call wins.
pub fn set_clipboard(backend: Arc<dyn Clipboard>) {
    let _ = CLIPBOARD.set(backend);
}

/// The installed backend, or `None` where there is no clipboard — headless, and Android today.
pub fn clipboard() -> Option<Arc<dyn Clipboard>> {
    CLIPBOARD.get().cloned()
}

/// The clipboard's text, or `None` with no backend installed. The spelling a widget reaches for.
pub fn clipboard_text() -> Option<String> {
    clipboard().and_then(|c| c.text())
}

/// Puts `text` on the clipboard if there is one. A no-op otherwise, which is the honest answer for a headless run: nothing to copy to.
pub fn set_clipboard_text(text: &str) {
    if let Some(c) = clipboard() {
        c.set_text(text);
    }
}

#[cfg(test)]
#[path = "clipboard_test.rs"]
mod tests;
