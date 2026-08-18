//! The system clipboard, as the vocabulary crate sees it.

use std::sync::{Arc, OnceLock};

/// Reading and writing the system selection.
///
/// `Send + Sync` for the same reason [`FileDialogs`](crate::dialogs::FileDialogs) is: a backend may have to
/// serve the bytes from a thread of its own. Wayland in particular hands the compositor a *source* rather than
/// a copy, and then serves whoever pastes — possibly minutes later — so a backend that owns a selection owns a
/// thread with it.
///
/// Both methods can fail quietly: a headless session has no clipboard, and a compositor can refuse. A caller
/// that pastes gets `None` and leaves the text alone, which is what every editor does when the selection is
/// empty anyway.
pub trait Clipboard: Send + Sync + 'static {
    /// The text currently on the clipboard, or `None` when it holds nothing this can read.
    fn text(&self) -> Option<String>;
    /// Puts `text` on the clipboard, replacing whatever was there.
    fn set_text(&self, text: &str);
}

static CLIPBOARD: OnceLock<Arc<dyn Clipboard>> = OnceLock::new();

/// Installs the backend the app's clipboard goes through. The desktop runner calls this at startup; a shell
/// that speaks the protocol itself installs its own, and a test can install a stub. The first call wins.
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

/// Puts `text` on the clipboard if there is one. A no-op otherwise, which is the honest answer for a headless
/// run: nothing to copy to.
pub fn set_clipboard_text(text: &str) {
    if let Some(c) = clipboard() {
        c.set_text(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Held(std::sync::Mutex<String>);

    impl Clipboard for Held {
        fn text(&self) -> Option<String> {
            Some(self.0.lock().unwrap().clone())
        }
        fn set_text(&self, text: &str) {
            *self.0.lock().unwrap() = text.to_owned();
        }
    }

    /// With no backend, a paste reads `None` and a copy is a no-op — a headless run must not panic on either,
    /// because the widget path that reaches them is the same one a test drives.
    #[test]
    fn no_backend_is_an_answer_rather_than_a_panic() {
        if clipboard().is_none() {
            assert_eq!(clipboard_text(), None);
            set_clipboard_text("dropped on the floor");
        }
    }

    #[test]
    fn an_installed_backend_round_trips() {
        set_clipboard(Arc::new(Held(std::sync::Mutex::new(String::new()))));
        set_clipboard_text("copied");
        assert_eq!(clipboard_text().as_deref(), Some("copied"));
    }
}
