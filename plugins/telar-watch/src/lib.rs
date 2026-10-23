//! Filesystem watching that arrives on the UI thread: [`watch_path`] calls back on the thread that owns the signals whenever a file, or a directory and everything under it, changes.
//!
//! The hard half of "reload when this file changes" is not noticing the change — `notify` does that — it is getting the notification onto the thread that owns the signals, since the watcher calls back from its own. Every app that wanted it wrote that bridge again, and one of them settled for polling mtime on a timer rather than build it.
//!
//! [`telar::spawn_stream`] is the bridge, already in the facade for exactly this shape: many values from a worker, each callback run on the UI thread during a later frame's `drain_tasks`. This crate is written against that public seam alone, the way an application's own code could be.
//!
//! An application depends on it beside `telar`:
//!
//! ```toml
//! # Cargo.toml
//! telar-watch = "0.2.2"
//! ```
//!
//! It ships no `.rsx` tags, so there is nothing to list in `telar.toml`.
//!
//! **Keep this crate on the same version as `telar`.** The [`Task`] [`watch_path`] returns is the facade's own, so a mismatch resolves two copies of the kernel and the task stops being the type `telar` hands out — a type error naming one type twice. It is the same lockstep `telar` and `telar-macros` already have, and for the same reason.

#![warn(rustdoc::broken_intra_doc_links)]

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use telar::{Emitter, Task, spawn_stream};

/// One editor save is several filesystem events — a truncate, a write, a rename into place — and a directory copy is thousands. Events are collected until this long has passed with none, so a caller reloads once.
const COALESCE: Duration = Duration::from_millis(50);

/// How long the worker waits before checking whether it has been cancelled. It is parked the rest of the time; this only bounds how long a retired watcher's thread outlives the [`Task`] that owned it.
const CANCEL_POLL: Duration = Duration::from_millis(500);

/// Calls `on_change` on **this** thread whenever `path` changes — a file, or a directory and everything under it. Coalesced, so one save is one call however many events the platform reported.
///
/// A file is watched through its parent directory, so it survives atomic saves (write a temp file, rename over it) and may not exist yet: its creation is a change. If the parent directory does not exist either, the watch cannot start and a `tracing` warning says why. A watcher error or a lost-events rescan is also reported as a change, since events may have been missed.
///
/// The returned [`Task`] owns the watch: keep it for as long as the reload should happen, and drop or [`cancel`](Task::cancel) it to stop. Dropping it detaches instead — the watcher keeps running and the callback keeps firing — which is what a watch that should outlive its setup function wants.
///
/// `on_change` takes no argument on purpose. What changed is a question with a different answer on every platform (and no answer at all for a coalesced batch), while *something under here changed, re-read it* is the same everywhere and is what a reloading caller acts on.
///
/// ```ignore
/// let _watch = telar_watch::watch_path(config_dir, move || settings.set(load_settings()));
/// ```
pub fn watch_path(path: impl Into<PathBuf>, mut on_change: impl FnMut() + 'static) -> Task {
    let path = path.into();
    spawn_stream(move |out| run(&path, out), move |()| on_change(), || {})
}

/// Whether an event is worth looking at the tree for. `Access` is the class inotify opens and reads under, so dropping it keeps a caller that re-reads on every change from waking itself forever.
///
/// A filter, not the answer: only inotify classifies that finely. FSEvents coalesces and re-labels, so what a read looks like there is decided by [`fingerprint`] instead.
fn worth_checking(event: &Event) -> bool {
    !matches!(event.kind, EventKind::Access(_))
}

/// What a change moves and a look does not: every path under `path`, with its length and its modification time. Reading a file leaves all three alone, so two equal fingerprints mean nothing happened however the platform labelled its event.
///
/// Cheap because it stats rather than reads, and only reached after a burst of events — which is already the moment the caller re-reads the tree anyway.
fn fingerprint(path: &Path) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
    fn walk(path: &Path, out: &mut Vec<(PathBuf, u64, Option<SystemTime>)>) {
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            return;
        };
        if meta.is_dir() {
            let Ok(entries) = std::fs::read_dir(path) else {
                return;
            };
            for entry in entries.flatten() {
                walk(&entry.path(), out);
            }
            return;
        }
        out.push((path.to_path_buf(), meta.len(), meta.modified().ok()));
    }
    let mut out = Vec::new();
    walk(path, &mut out);
    // `read_dir` answers in whatever order the filesystem holds, which is not stable across calls.
    out.sort();
    out
}

enum Signal {
    Touched,
    Lost,
}

/// Where the platform watch goes: a directory is watched recursively, anything else through its parent directory, because an atomic save replaces the file's inode and a watch on the inode dies with it.
struct Target {
    watched: PathBuf,
    mode: RecursiveMode,
    file_name: Option<OsString>,
}

fn target(path: &Path) -> Result<Target, String> {
    if path.is_dir() {
        return Ok(Target {
            watched: path.to_path_buf(),
            mode: RecursiveMode::Recursive,
            file_name: None,
        });
    }
    let Some(file_name) = path.file_name() else {
        return Err("it has no file name".to_owned());
    };
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    if !parent.is_dir() {
        return Err(format!("its directory {} does not exist", parent.display()));
    }
    Ok(Target {
        watched: parent.to_path_buf(),
        mode: RecursiveMode::NonRecursive,
        file_name: Some(file_name.to_owned()),
    })
}

fn names_file(event: &Event, file_name: &OsStr) -> bool {
    event.paths.iter().any(|p| p.file_name() == Some(file_name))
}

fn run(path: &Path, out: Emitter<()>) {
    let target = match target(path) {
        Ok(target) => target,
        Err(reason) => {
            tracing::warn!("cannot watch {}: {reason}", path.display());
            return;
        }
    };
    let (tx, rx) = mpsc::channel();
    let file_name = target.file_name.clone();
    let watched = target.watched.clone();
    let mut watcher: RecommendedWatcher = match notify::recommended_watcher(
        move |result: notify::Result<Event>| {
            let signal = match result {
                Ok(event) if event.need_rescan() => Signal::Lost,
                Ok(event) if !worth_checking(&event) => return,
                Ok(event) => match &file_name {
                    Some(name) if !names_file(&event, name) => return,
                    _ => Signal::Touched,
                },
                Err(e) => {
                    tracing::warn!(
                        "watcher error under {}: {e}; if this is the inotify limit, raise fs.inotify.max_user_watches",
                        watched.display()
                    );
                    Signal::Lost
                }
            };
            let _ = tx.send(signal);
        },
    ) {
        Ok(watcher) => watcher,
        Err(e) => {
            tracing::warn!("cannot watch {}: {e}", path.display());
            return;
        }
    };
    if let Err(e) = watcher.watch(&target.watched, target.mode) {
        tracing::warn!(
            "cannot watch {}: {e}; if this is the inotify limit, raise fs.inotify.max_user_watches",
            path.display()
        );
        return;
    }

    let mut seen = fingerprint(path);
    loop {
        let mut lost = match rx.recv_timeout(CANCEL_POLL) {
            Ok(signal) => matches!(signal, Signal::Lost),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if out.is_cancelled() {
                    return;
                }
                continue;
            }
            // The watcher was dropped, which only happens on the way out of this function.
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
        // Drain the rest of the burst before reporting, so a save that lands as four events is one reload.
        while let Ok(signal) = rx.recv_timeout(COALESCE) {
            lost |= matches!(signal, Signal::Lost);
        }
        if out.is_cancelled() {
            return;
        }
        let now = fingerprint(path);
        if now == seen && !lost {
            continue;
        }
        seen = now;
        out.emit(());
    }
}

#[cfg(test)]
#[path = "watch_test.rs"]
mod tests;
