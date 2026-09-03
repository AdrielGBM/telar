//! Where a browser build's log lines go.
//!
//! `tracing` with no subscriber discards everything, and the default `fmt` subscriber writes to stdout,
//! which a page does not have. Without this a device that fails to open, or a frame that fails to present,
//! reports itself into nothing — which is the one situation where the log is the only thing there is.

use std::io::Write;

use tracing_subscriber::fmt::MakeWriter;

/// Buffers one formatted event and hands it to the browser console when the writer is dropped, because that
/// is the granularity the console shows: a partial write would appear as its own line.
struct ConsoleWriter {
    line: Vec<u8>,
}

impl Write for ConsoleWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.line.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for ConsoleWriter {
    fn drop(&mut self) {
        let text = String::from_utf8_lossy(&self.line);
        let text = text.trim_end();
        if !text.is_empty() {
            web_sys::console::log_1(&text.into());
        }
    }
}

struct MakeConsoleWriter;

impl<'a> MakeWriter<'a> for MakeConsoleWriter {
    type Writer = ConsoleWriter;

    fn make_writer(&'a self) -> Self::Writer {
        ConsoleWriter { line: Vec::new() }
    }
}

/// Sends `tracing` output to the browser console, and a panic's message and backtrace with it.
///
/// Does nothing if a subscriber is already installed, so an application that wants its own keeps it.
pub fn install_console_logging() {
    console_error_panic_hook::set_once();
    let _ = tracing_subscriber::fmt()
        .with_writer(MakeConsoleWriter)
        // A browser console stamps its own times, and ANSI colour codes print as escape sequences there.
        .without_time()
        .with_ansi(false)
        .try_init();
}
