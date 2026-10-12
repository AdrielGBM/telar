//! The running workshop's channel: how a later `cargo telar preview`, or the editor, reaches the workshop a session already runs for the package, instead of opening a second one.
//!
//! The session that runs a workshop in a window listens on a loopback port from the moment it starts, before anything is built, and writes it to `<workspace>/.telar/workshop/<package>.channel`. A request is one line, `goto:<location>` or `focus`, answered `ok` once the session has taken it. What arrives before the workshop is up waits for it, and a later location replaces one still waiting. Anything else — no file, a port nobody listens on, a listener that never answers — means no workshop is running, and the invocation opens one.
//!
//! The session serving the workshop as a page writes `starting <pid>` to `<package>.server` as it launches and its address once it serves, and is reached in a browser instead: the address answers for as long as the page is served. A session that fails removes the file; one that is killed leaves its pid for the editor to find dead.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use telar_project::protocol::{
    FOCUS_LINE, GOTO_PREFIX, ServerFile, parse_server_file, preview_location, starting_server_line,
};

const ANSWER_TIMEOUT: Duration = Duration::from_secs(2);
const ACCEPTED: &str = "ok";
const REFUSED: &str = "no";

/// What `cargo telar preview <target>` asks the workshop to open: a link it copied, which already is a location, or a preview's id.
pub(crate) fn location_of(target: &str) -> String {
    if is_preview_id(target) {
        preview_location(target)
    } else {
        target.to_string()
    }
}

/// Whether `target` is a preview's id rather than a copied link.
pub(crate) fn is_preview_id(target: &str) -> bool {
    !target.starts_with('/')
}

/// Asks the workshop the channel at `file` names to open `location`, and answers whether it took the request.
pub(crate) fn goto(file: &Path, location: &str) -> bool {
    request(file, &format!("{GOTO_PREFIX}{location}"))
}

/// Asks the workshop the channel at `file` names to bring its window forward, and answers whether it took the request.
pub(crate) fn focus(file: &Path) -> bool {
    request(file, FOCUS_LINE)
}

/// Sends `line` to the session the channel at `file` names. A file left behind by a session that is gone is removed.
fn request(file: &Path, line: &str) -> bool {
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
    ask(stream, line)
}

fn ask(mut stream: TcpStream, line: &str) -> bool {
    if stream.set_read_timeout(Some(ANSWER_TIMEOUT)).is_err() || writeln!(stream, "{line}").is_err()
    {
        return false;
    }
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer).is_ok() && answer.trim() == ACCEPTED
}

fn is_request(line: &str) -> bool {
    line.starts_with(GOTO_PREFIX) || line == FOCUS_LINE
}

/// The request to hand the workshop once it can take one, after `line` arrives with `waiting` not yet handed over: the newer, except that a `focus` does not replace a location, which brings the window forward too.
pub(crate) fn supersede(waiting: Option<String>, line: String) -> Option<String> {
    match waiting {
        Some(waiting) if line == FOCUS_LINE => Some(waiting),
        _ => Some(line),
    }
}

/// The listening half, held by the session running a workshop in a window. Requests are answered on a thread of its own, so one sent while the session is still building is taken rather than left to time out. Dropping it removes the file that names the port.
pub(crate) struct WorkshopChannel {
    requests: mpsc::Receiver<String>,
    file: PathBuf,
}

impl WorkshopChannel {
    /// Listens on a free loopback port and names it in `file`. `None` when either fails: the workshop runs on, and a later invocation opens a window of its own.
    pub(crate) fn open(file: PathBuf) -> Option<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
        let port = listener.local_addr().ok()?.port();
        std::fs::create_dir_all(file.parent()?).ok()?;
        std::fs::write(&file, format!("{port}\n")).ok()?;
        let (taken, requests) = mpsc::channel();
        std::thread::Builder::new()
            .name("telar-workshop-channel".to_string())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    if !answer(stream, &taken) {
                        return;
                    }
                }
            })
            .ok()?;
        Some(Self { requests, file })
    }

    /// `waiting` with every request taken since folded in by [`supersede`]. Never blocks.
    pub(crate) fn take(&self, waiting: Option<String>) -> Option<String> {
        self.requests.try_iter().fold(waiting, supersede)
    }
}

/// Reads one request from `stream`, passes it to `taken` and answers whether it was. `false` once the session has stopped taking requests.
fn answer(stream: TcpStream, taken: &mpsc::Sender<String>) -> bool {
    if stream.set_read_timeout(Some(ANSWER_TIMEOUT)).is_err() {
        return true;
    }
    let Ok(mut writer) = stream.try_clone() else {
        return true;
    };
    let mut line = String::new();
    if BufReader::new(stream).read_line(&mut line).is_err() {
        return true;
    }
    let line = line.trim_end_matches(['\r', '\n']);
    let (accepted, open) = match is_request(line) {
        true => match taken.send(line.to_string()) {
            Ok(()) => (true, true),
            Err(_) => (false, false),
        },
        false => (false, true),
    };
    let _ = writeln!(writer, "{}", if accepted { ACCEPTED } else { REFUSED });
    open
}

impl Drop for WorkshopChannel {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

/// Names `address` in `file` for the session that has just started serving the workshop there. Every other server file naming the same address is removed: the session could only listen there because nothing else does, so those were left by sessions that are gone.
pub(crate) fn announce_server(file: &Path, address: &str) {
    let Some(dir) = file.parent() else {
        return;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let stale = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path != file && path.extension().is_some_and(|ext| ext == "server"))
        .filter(|path| std::fs::read_to_string(path).is_ok_and(|named| named.trim() == address));
    for path in stale {
        let _ = std::fs::remove_file(path);
    }
    let _ = std::fs::write(file, format!("{address}\n"));
}

/// Marks `file` as the server file of a session that has launched and serves nothing yet.
pub(crate) fn announce_starting(file: &Path) {
    let Some(dir) = file.parent() else {
        return;
    };
    if std::fs::create_dir_all(dir).is_ok() {
        let line = starting_server_line(std::process::id());
        let _ = std::fs::write(file, format!("{line}\n"));
    }
}

/// Removes the server file of a session that is giving up.
pub(crate) fn retire_server(file: &Path) {
    let _ = std::fs::remove_file(file);
}

/// The address the server file at `file` names, when a session still serves the workshop there. A file left behind by a session that is gone is removed; one of a session that is still starting is left alone.
pub(crate) fn serving(file: &Path) -> Option<String> {
    let Some(ServerFile::Serving(address)) =
        parse_server_file(&std::fs::read_to_string(file).ok()?)
    else {
        return None;
    };
    if answers(&address) {
        Some(address)
    } else {
        let _ = std::fs::remove_file(file);
        None
    }
}

/// Whether a server at `address`, an `http://` address on this machine, answers the build counter every page served for development polls.
fn answers(address: &str) -> bool {
    let Some(port) = address
        .strip_prefix("http://")
        .and_then(|rest| rest.split('/').next())
        .and_then(|authority| authority.rsplit_once(':'))
        .and_then(|(_, port)| port.parse::<u16>().ok())
    else {
        return false;
    };
    let at = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = TcpStream::connect_timeout(&at, ANSWER_TIMEOUT) else {
        return false;
    };
    if stream.set_read_timeout(Some(ANSWER_TIMEOUT)).is_err()
        || stream
            .write_all(b"GET /telar-build HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n")
            .is_err()
    {
        return false;
    }
    let mut status = [0; 12];
    stream.read_exact(&mut status).is_ok() && status.ends_with(b" 200")
}

#[cfg(test)]
#[path = "workshop_channel_test.rs"]
mod tests;
