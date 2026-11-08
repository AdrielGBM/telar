use std::sync::Arc;

use super::*;

#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

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
    let clipboard = OscClipboard::new(SharedBuffer::default());
    assert_eq!(clipboard.text(), None);
    clipboard.set_text("hola");
    assert_eq!(clipboard.text().as_deref(), Some("hola"));
}

#[test]
fn copying_asks_the_terminal_with_one_osc_52_sequence() {
    let out = SharedBuffer::default();
    let clipboard = OscClipboard::new(out.clone());
    clipboard.set_text("hola");
    assert_eq!(out.0.lock().unwrap().as_slice(), b"\x1b]52;c;aG9sYQ==\x07");
}
