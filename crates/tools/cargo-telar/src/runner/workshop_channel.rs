//! The running workshop's channel: how a later `cargo telar preview`, or the editor, reaches the workshop a session already runs for the package, instead of opening a second one.
//!
//! The session that runs a workshop in a window listens on a loopback port from the moment it starts, before anything is built, and writes it to `<workspace>/.telar/workshop/<package>.channel`. A request is one line, `goto:<location>` or `focus`, answered `ok` once the session has taken it. What arrives before the workshop is up waits for it, and a later location replaces one still waiting. Anything else — no file, a port nobody listens on, a listener that never answers — means no workshop is running, and the invocation opens one.
//!
//! The session serving the workshop as a page is reached in a browser instead. Before it builds anything it claims `<package>.server` by locking `<package>.server.lock` for as long as it runs, and writes `starting <pid>` there, then its address once it serves, which answers for as long as the page is served. A later invocation that finds the lock held says where that session serves, or that it is starting, instead of starting a second one. Both files naming a session are removed when it ends, interrupted or not; one that is killed outright leaves its pid for the editor to find dead, and its lock released.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use telar_project::protocol::{
    FOCUS_LINE, GOTO_PREFIX, ServerFile, parse_server_file, preview_location, starting_server_line,
};

use super::interrupt::{keep_on_interrupt, remove_on_interrupt};

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

/// The listening half, held by the session running a workshop in a window. Requests are answered on a thread of its own, so one sent while the session is still building is taken rather than left to time out. Dropping it removes the file that names the port, and so does an interruption.
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
        let (taken, requests) = mpsc::channel();
        remove_on_interrupt(&file);
        let channel = Self { requests, file };
        std::fs::write(&channel.file, format!("{port}\n")).ok()?;
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
        Some(channel)
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
        keep_on_interrupt(&self.file);
    }
}

/// The hold a session serving the workshop as a page has on the package's server file, from before it builds anything until it ends: no other session serves the package's workshop meanwhile. Dropping it removes the file, and an interruption removes it too.
pub(crate) struct ServerClaim {
    file: PathBuf,
    _lock: Option<File>,
}

/// The live session another one found serving the package's workshop as a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ServedElsewhere {
    /// Serving at this address.
    Serving(String),
    /// Still building, as process `pid` once its file names it.
    Starting { pid: Option<u32> },
}

impl ServerClaim {
    /// Claims `file` for this session and marks it starting, or says which live session holds it. Where no lock can be taken at all, the session runs on unguarded rather than not at all.
    pub(crate) fn claim(file: &Path) -> Result<Self, ServedElsewhere> {
        let lock = match lock_server_file(file) {
            Ok(lock) => lock,
            Err(TryLockError::WouldBlock) => return Err(held_by(file)),
            Err(TryLockError::Error(_)) => None,
        };
        remove_on_interrupt(file);
        if let Some(dir) = file.parent()
            && std::fs::create_dir_all(dir).is_ok()
        {
            let line = starting_server_line(std::process::id());
            let _ = std::fs::write(file, format!("{line}\n"));
        }
        Ok(Self {
            file: file.to_path_buf(),
            _lock: lock,
        })
    }

    /// Names `address` in the file once the session serves the workshop there. Every other server file naming the same address is removed: the session could only listen there because nothing else does, so those were left by sessions that are gone.
    pub(crate) fn serve(&self, address: &str) {
        let Some(dir) = self.file.parent() else {
            return;
        };
        let stale = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                *path != self.file && path.extension().is_some_and(|ext| ext == "server")
            })
            .filter(|path| {
                std::fs::read_to_string(path).is_ok_and(|named| named.trim() == address)
            });
        for path in stale {
            let _ = std::fs::remove_file(path);
        }
        let _ = std::fs::write(&self.file, format!("{address}\n"));
    }
}

impl Drop for ServerClaim {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
        keep_on_interrupt(&self.file);
    }
}

/// Locks the file guarding the server file at `file`, held for as long as the returned file is open and released by the system when the process ends, however it ends. `Ok(None)` where no lock file can be made.
fn lock_server_file(file: &Path) -> Result<Option<File>, TryLockError> {
    let opened = file
        .parent()
        .filter(|dir| std::fs::create_dir_all(dir).is_ok())
        .and_then(|_| {
            OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(file.with_extension("server.lock"))
                .ok()
        });
    let Some(lock) = opened else {
        return Ok(None);
    };
    lock.try_lock()?;
    Ok(Some(lock))
}

/// What the server file at `file` says of the live session holding its lock. An address that does not answer is one that session has not overwritten yet, or is about to serve.
fn held_by(file: &Path) -> ServedElsewhere {
    let named = std::fs::read_to_string(file)
        .ok()
        .and_then(|text| parse_server_file(&text));
    match named {
        Some(ServerFile::Serving(address)) if answers(&address) => {
            ServedElsewhere::Serving(address)
        }
        Some(ServerFile::Starting { pid }) => ServedElsewhere::Starting { pid: Some(pid) },
        _ => ServedElsewhere::Starting { pid: None },
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
