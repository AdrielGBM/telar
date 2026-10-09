//! The running workshop's channel: how `cargo telar preview <ID>` reaches the workshop another session of it is already showing, instead of opening a second window.
//!
//! The session that runs a workshop under hot reload listens on a loopback port and writes it to `<workspace>/.telar/workshop/<package>.channel`. A later invocation for the same package reads the port, sends the location to open as one `goto:<location>` line, and is answered `ok` once the session has passed that line on to the workshop over the hot-reload channel. Anything else — no file, a port nobody listens on, a listener that never answers — means no workshop is running, and the invocation opens one.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use telar_project::protocol::GOTO_PREFIX;

const ANSWER_TIMEOUT: Duration = Duration::from_secs(2);
const ACCEPTED: &str = "ok";
const REFUSED: &str = "no";

/// Where the session running `package`'s workshop writes its port.
pub(crate) fn channel_file(workspace_root: &Path, package: &str) -> PathBuf {
    workspace_root
        .join(".telar")
        .join("workshop")
        .join(format!("{package}.channel"))
}

/// What `cargo telar preview <target>` asks the workshop to open: a link it copied, which already is a location, or a preview's id.
pub(crate) fn location_of(target: &str) -> String {
    if target.starts_with('/') {
        return target.to_string();
    }
    format!("/preview/{}", encode_segment(target))
}

/// Whether `target` is a preview's id rather than a copied link.
pub(crate) fn is_preview_id(target: &str) -> bool {
    !target.starts_with('/')
}

fn encode_segment(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'=' | b'+') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Asks the workshop the channel at `file` names to open `location`, and answers whether it said it would. A file left behind by a session that is gone is removed.
pub(crate) fn send(file: &Path, location: &str) -> bool {
    let Some(port) = std::fs::read_to_string(file)
        .ok()
        .and_then(|text| text.trim().parse::<u16>().ok())
    else {
        return false;
    };
    let Ok(stream) = TcpStream::connect((Ipv4Addr::LOCALHOST, port)) else {
        let _ = std::fs::remove_file(file);
        return false;
    };
    ask(stream, location)
}

fn ask(mut stream: TcpStream, location: &str) -> bool {
    if stream.set_read_timeout(Some(ANSWER_TIMEOUT)).is_err()
        || writeln!(stream, "{GOTO_PREFIX}{location}").is_err()
    {
        return false;
    }
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer).is_ok() && answer.trim() == ACCEPTED
}

/// The listening half, held by the session running a workshop. Dropping it closes the port and removes the file that named it.
pub(crate) struct WorkshopChannel {
    listener: TcpListener,
    file: PathBuf,
}

impl WorkshopChannel {
    /// Listens on a free loopback port and names it in `file`. `None` when either fails: the workshop runs on, and a later invocation opens a window of its own.
    pub(crate) fn open(file: PathBuf) -> Option<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
        listener.set_nonblocking(true).ok()?;
        let port = listener.local_addr().ok()?.port();
        std::fs::create_dir_all(file.parent()?).ok()?;
        std::fs::write(&file, format!("{port}\n")).ok()?;
        Some(Self { listener, file })
    }

    /// Takes every request waiting, hands each `goto:` line to `forward`, and answers whether `forward` passed it on. Never blocks on a connection that has not arrived.
    pub(crate) fn poll(&self, mut forward: impl FnMut(&str) -> bool) {
        while let Ok((stream, _)) = self.listener.accept() {
            answer(stream, &mut forward);
        }
    }
}

fn answer(stream: TcpStream, forward: &mut impl FnMut(&str) -> bool) {
    // An accepted stream inherits the listener's non-blocking mode on some platforms.
    if stream.set_nonblocking(false).is_err()
        || stream.set_read_timeout(Some(ANSWER_TIMEOUT)).is_err()
    {
        return;
    }
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut line = String::new();
    if BufReader::new(stream).read_line(&mut line).is_err() {
        return;
    }
    let line = line.trim_end_matches(['\r', '\n']);
    let accepted = line.starts_with(GOTO_PREFIX) && forward(line);
    let _ = writeln!(writer, "{}", if accepted { ACCEPTED } else { REFUSED });
}

impl Drop for WorkshopChannel {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

#[cfg(test)]
#[path = "workshop_channel_test.rs"]
mod tests;
