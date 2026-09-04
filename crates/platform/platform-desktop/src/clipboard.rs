//! The desktop clipboard, behind the shared service trait.

use std::sync::Mutex;

use services_core::Clipboard;

/// The desktop clipboard, over `arboard`.
///
/// The handle is kept rather than opened per call: on X11 and Wayland a clipboard *owner* has to stay alive to serve the bytes to whoever pastes, so a handle dropped after `set_text` takes the selection with it. Behind a `Mutex` because the trait is `Send + Sync` and `arboard`'s handle is not.
pub struct DesktopClipboard(Mutex<Option<arboard::Clipboard>>);

impl DesktopClipboard {
    /// Opens the platform clipboard, or reports why not. Failing here is normal — a headless session has none.
    pub fn new() -> Result<Self, arboard::Error> {
        Ok(Self(Mutex::new(Some(arboard::Clipboard::new()?))))
    }

    /// Installs this as the app's clipboard, logging and carrying on where the platform has none: a missing clipboard is a paste that does nothing, not a startup that fails.
    pub fn install() {
        match Self::new() {
            Ok(clipboard) => services_core::set_clipboard(std::sync::Arc::new(clipboard)),
            Err(e) => tracing::warn!("no system clipboard: {e}"),
        }
    }

    fn with<R>(&self, f: impl FnOnce(&mut arboard::Clipboard) -> R) -> Option<R> {
        self.0.lock().ok()?.as_mut().map(f)
    }
}

impl Clipboard for DesktopClipboard {
    fn text(&self) -> Option<String> {
        self.with(|c| c.get_text().ok())?
    }

    fn set_text(&self, text: &str) {
        if let Some(Err(e)) = self.with(|c| c.set_text(text.to_owned())) {
            tracing::warn!("could not set the clipboard: {e}");
        }
    }
}
