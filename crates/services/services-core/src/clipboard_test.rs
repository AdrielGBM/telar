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

/// With no backend, a paste reads `None` and a copy is a no-op — a headless run must not panic on either, because the widget path that reaches them is the same one a test drives. Then the installed backend round-trips.
///
/// One test rather than two, because `CLIPBOARD` is a process-wide `OnceLock` and the harness runs tests concurrently: split in two, the empty case reads `None`, the other test installs a backend, and the empty case's next line reads `Some`. Ordering them is the fix, and one test is how you order them.
#[test]
fn a_paste_answers_with_a_backend_and_without_one() {
    assert_eq!(clipboard_text(), None, "nothing installed yet");
    set_clipboard_text("dropped on the floor");

    set_clipboard(Arc::new(Held(std::sync::Mutex::new(String::new()))));
    set_clipboard_text("copied");
    assert_eq!(clipboard_text().as_deref(), Some("copied"));
}
