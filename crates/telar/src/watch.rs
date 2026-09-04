//! Filesystem watching that arrives on the UI thread.
//!
//! The hard half of "reload when this file changes" is not noticing the change — `notify` does that — it is getting the notification onto the thread that owns the signals, since the watcher calls back from its own. Every app that wanted it wrote that bridge again, and one of them settled for polling mtime on a timer rather than build it.
//!
//! [`reactive_core::spawn_stream`] is the bridge, already here for exactly this shape: many values from a worker, each callback run on the UI thread during a later frame's `drain_tasks`.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use reactive_core::{Emitter, Task, spawn_stream};

/// One editor save is several filesystem events — a truncate, a write, a rename into place — and a directory copy is thousands. Events are collected until this long has passed with none, so a caller reloads once.
const COALESCE: Duration = Duration::from_millis(50);

/// How long the worker waits before checking whether it has been cancelled. It is parked the rest of the time; this only bounds how long a retired watcher's thread outlives the [`Task`] that owned it.
const CANCEL_POLL: Duration = Duration::from_millis(500);

/// Calls `on_change` on **this** thread whenever `path` changes — a file, or a directory and everything under it. Coalesced, so one save is one call however many events the platform reported.
///
/// The returned [`Task`] owns the watch: keep it for as long as the reload should happen, and drop or [`cancel`](Task::cancel) it to stop. Dropping it detaches instead — the watcher keeps running and the callback keeps firing — which is what a watch that should outlive its setup function wants.
///
/// `on_change` takes no argument on purpose. What changed is a question with a different answer on every platform (and no answer at all for a coalesced batch), while *something under here changed, re-read it* is the same everywhere and is what a reloading caller acts on.
///
/// ```ignore
/// let _watch = telar::watch_path(config_dir, move || settings.set(load_settings()));
/// ```
pub fn watch_path(path: impl Into<PathBuf>, mut on_change: impl FnMut() + 'static) -> Task {
    let path = path.into();
    spawn_stream(move |out| run(&path, out), move |()| on_change(), || {})
}

/// Whether an event is the tree changing rather than somebody looking at it.
///
/// **Reads are not changes, and the platform reports them.** `notify` asks inotify for `IN_OPEN` alongside the writes, so a watcher that forwards every event it is handed tells a caller that re-reads the tree to re-read the tree — and one reading on a frame loop never stops. Every content change arrives as a create, a modify or a remove, a rename into place among them; what is dropped here is `Access`, which is the class the platform opens and reads under.
fn changed(event: &Event) -> bool {
    !matches!(event.kind, EventKind::Access(_))
}

fn run(path: &Path, out: Emitter<()>) {
    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher =
        match notify::recommended_watcher(move |result: notify::Result<Event>| {
            if result.is_ok_and(|event| changed(&event)) {
                let _ = tx.send(());
            }
        }) {
            Ok(watcher) => watcher,
            Err(e) => {
                tracing::warn!("cannot watch {}: {e}", path.display());
                return;
            }
        };
    // Recursive whether `path` is a directory or a file: watching a file non-recursively is the same thing, and it saves the caller a `is_dir()` that races with the change it is asking to be told about.
    if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
        tracing::warn!("cannot watch {}: {e}", path.display());
        return;
    }

    loop {
        match rx.recv_timeout(CANCEL_POLL) {
            Ok(()) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if out.is_cancelled() {
                    return;
                }
                continue;
            }
            // The watcher was dropped, which only happens on the way out of this function.
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        // Drain the rest of the burst before reporting, so a save that lands as four events is one reload.
        while rx.recv_timeout(COALESCE).is_ok() {}
        if out.is_cancelled() {
            return;
        }
        out.emit(());
    }
}
