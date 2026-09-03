//! The system clipboard, over the wire.
//!
//! A terminal application has no connection to the display server, so it cannot read or write the
//! clipboard directly. OSC 52 asks the *terminal emulator* to do it — which is the only mechanism that
//! works over SSH as well as locally, and is why it is worth having despite what it cannot do.

use std::io::Write;
use std::sync::Mutex;

use services_core::Clipboard;

/// Copies through the terminal, and reads back what this application last copied.
///
/// Reading the real clipboard would mean an OSC 52 query, whose answer arrives as *input* — interleaved
/// with the user's keystrokes, on a terminal that may never reply at all, and which most emulators disable
/// by default because it lets any program read the user's clipboard. So a paste of something copied
/// elsewhere reaches the app the way it does in every other terminal program: the terminal's own paste,
/// which arrives as bracketed-paste input.
pub struct OscClipboard {
    last_copied: Mutex<String>,
}

impl OscClipboard {
    pub fn new() -> Self {
        Self {
            last_copied: Mutex::new(String::new()),
        }
    }
}

impl Default for OscClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Clipboard for OscClipboard {
    fn text(&self) -> Option<String> {
        let text = self.last_copied.lock().ok()?;
        (!text.is_empty()).then(|| text.clone())
    }

    fn set_text(&self, text: &str) {
        if let Ok(mut last) = self.last_copied.lock() {
            last.clear();
            last.push_str(text);
        }
        let mut out = std::io::stdout();
        let _ = write!(out, "\x1b]52;c;{}\x07", base64(text.as_bytes()));
        let _ = out.flush();
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn what_was_copied_reads_back() {
        let clipboard = OscClipboard::new();
        assert_eq!(clipboard.text(), None);
        clipboard.set_text("hola");
        assert_eq!(clipboard.text().as_deref(), Some("hola"));
    }
}
