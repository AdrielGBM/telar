use super::*;
use std::sync::mpsc;
use std::thread;

fn stat(name: &'static str, bytes: usize, entries: usize, capacity: usize) -> CacheStat {
    CacheStat {
        name,
        bytes,
        entries,
        capacity,
    }
}

fn reported(name: &str) -> Option<CacheStat> {
    snapshot().into_iter().find(|stat| stat.name == name)
}

fn publish_from_thread(stats: Vec<CacheStat>) -> thread::JoinHandle<()> {
    thread::spawn(move || publish(stats))
}

#[test]
fn threads_reporting_the_same_cache_are_summed() {
    publish_from_thread(vec![stat("sum.raster", 100, 2, 1000)])
        .join()
        .unwrap();
    publish_from_thread(vec![stat("sum.raster", 50, 1, 1000)])
        .join()
        .unwrap();

    assert_eq!(
        reported("sum.raster"),
        Some(stat("sum.raster", 150, 3, 2000))
    );
}

#[test]
fn a_shared_name_published_from_two_threads_is_counted_once() {
    thread::spawn(|| publish_shared("shared.gpu", vec![stat("shared.texture", 80, 4, 500)]))
        .join()
        .unwrap();
    thread::spawn(|| publish_shared("shared.gpu", vec![stat("shared.texture", 80, 4, 500)]))
        .join()
        .unwrap();

    assert_eq!(
        reported("shared.texture"),
        Some(stat("shared.texture", 80, 4, 500))
    );
}

#[test]
fn a_republished_census_replaces_the_threads_previous_one() {
    thread::spawn(|| {
        publish(vec![stat("replace.cache", 10, 1, 100)]);
        publish(vec![stat("replace.cache", 30, 3, 100)]);
    })
    .join()
    .unwrap();

    assert_eq!(
        reported("replace.cache"),
        Some(stat("replace.cache", 30, 3, 100))
    );
}

#[test]
fn the_snapshot_is_sorted_by_name() {
    publish_from_thread(vec![
        stat("order.c", 1, 1, 1),
        stat("order.a", 1, 1, 1),
        stat("order.b", 1, 1, 1),
    ])
    .join()
    .unwrap();

    let names: Vec<&str> = snapshot()
        .iter()
        .map(|stat| stat.name)
        .filter(|name| name.starts_with("order."))
        .collect();
    assert_eq!(names, ["order.a", "order.b", "order.c"]);
}

#[test]
fn a_thread_is_due_once_and_then_not_again_within_the_interval() {
    let (first, second) = thread::spawn(|| (publish_due(), publish_due()))
        .join()
        .unwrap();

    assert!(first, "a thread that never published is due");
    assert!(!second, "and is not due again straight after");
}

#[test]
fn an_unpublished_thread_stops_being_counted() {
    let (published_tx, published_rx) = mpsc::channel();
    let (unpublish_tx, unpublish_rx) = mpsc::channel::<()>();
    let worker = thread::spawn(move || {
        publish(vec![stat("gone.cache", 7, 1, 70)]);
        published_tx.send(()).unwrap();
        unpublish_rx.recv().unwrap();
        unpublish();
    });

    published_rx.recv().unwrap();
    assert_eq!(reported("gone.cache"), Some(stat("gone.cache", 7, 1, 70)));

    unpublish_tx.send(()).unwrap();
    worker.join().unwrap();
    assert_eq!(reported("gone.cache"), None);
}

#[test]
fn unpublishing_leaves_other_threads_entries_alone() {
    publish_from_thread(vec![stat("kept.cache", 5, 1, 50)])
        .join()
        .unwrap();
    thread::spawn(unpublish).join().unwrap();

    assert_eq!(reported("kept.cache"), Some(stat("kept.cache", 5, 1, 50)));
}
