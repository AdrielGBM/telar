//! Integration test for `watch_path`: a real file change, a real platform watcher, and the callback landing on the thread that asked for it.
//!
//! Nothing here can be unit-tested — the whole point of the helper is the hop from the watcher's thread to the UI thread, and a test that stubbed the watcher would assert only that `spawn_stream` works.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use telar::{drain_tasks, watch_path};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("telar-watch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Drains delivered task values until `hits` stops rising, or gives up. Stands in for the runner's per-frame `drain_tasks`.
fn settle_until(hits: &Rc<Cell<usize>>, want: usize, timeout: Duration) -> usize {
    let deadline = Instant::now() + timeout;
    while hits.get() < want && Instant::now() < deadline {
        drain_tasks();
        std::thread::sleep(Duration::from_millis(10));
    }
    drain_tasks();
    hits.get()
}

#[test]
fn a_write_under_the_watched_directory_reaches_the_ui_thread() {
    let dir = scratch("write");
    let hits = Rc::new(Cell::new(0usize));

    let counted = Rc::clone(&hits);
    let watch = watch_path(&dir, move || counted.set(counted.get() + 1));

    // Give the platform watcher a moment to register before touching anything, or the change races the watch.
    std::thread::sleep(Duration::from_millis(200));
    std::fs::write(dir.join("config.toml"), "a = 1\n").unwrap();

    assert!(
        settle_until(&hits, 1, Duration::from_secs(10)) >= 1,
        "the write never reached the callback"
    );

    watch.cancel();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A cancelled watch stops calling back. Without this the `Task` would be decoration: the callback usually writes a signal belonging to state the caller is in the middle of tearing down.
#[test]
fn a_cancelled_watch_stops_reporting() {
    let dir = scratch("cancel");
    let hits = Rc::new(Cell::new(0usize));

    let counted = Rc::clone(&hits);
    let watch = watch_path(&dir, move || counted.set(counted.get() + 1));
    std::thread::sleep(Duration::from_millis(200));
    std::fs::write(dir.join("first.toml"), "a = 1\n").unwrap();
    let before = settle_until(&hits, 1, Duration::from_secs(10));
    assert!(before >= 1, "the watch never reported at all");

    watch.cancel();
    std::fs::write(dir.join("second.toml"), "b = 2\n").unwrap();
    std::thread::sleep(Duration::from_millis(300));
    drain_tasks();

    assert_eq!(hits.get(), before, "a cancelled watch kept reporting");
    let _ = std::fs::remove_dir_all(&dir);
}

/// **Reading a watched file is not a change to it.**
///
/// The platform reports opens and reads alongside the writes — inotify is asked for `IN_OPEN` — so a watcher that forwards every event it is handed answers "something changed" to a caller that was only looking. A caller that re-reads the tree when told to then tells itself to re-read it, and one reading on a frame loop never stops.
#[test]
fn reading_a_watched_file_is_not_a_change() {
    let dir = scratch("read");
    let file = dir.join("held.toml");
    std::fs::write(&file, "a = 1\n").unwrap();

    let hits = Rc::new(Cell::new(0usize));
    let counted = Rc::clone(&hits);
    let watch = watch_path(&dir, move || counted.set(counted.get() + 1));
    std::thread::sleep(Duration::from_millis(200));

    for _ in 0..20 {
        let _ = std::fs::read_to_string(&file).unwrap();
    }
    std::thread::sleep(Duration::from_millis(300));
    drain_tasks();
    assert_eq!(hits.get(), 0, "reading the file reported it as changed");

    // And the watch is still live: what it drops is the looking, not the watching.
    std::fs::write(&file, "a = 2\n").unwrap();
    assert!(
        settle_until(&hits, 1, Duration::from_secs(10)) >= 1,
        "the write after the reads never reached the callback"
    );

    watch.cancel();
    let _ = std::fs::remove_dir_all(&dir);
}
