use super::*;

use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::mpsc::{self, UnboundedReceiver};

use crate::rpc::OutgoingSender;

/// A backend with nothing loaded. The receiver comes back with it so the outgoing channel stays open for the length of the test — `OutgoingSender` drops messages once it closes.
fn unloaded() -> (Backend, UnboundedReceiver<Value>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (Backend::new(OutgoingSender::new(tx)), rx)
}

fn age(backend: &Backend) {
    let past = Instant::now()
        .checked_sub(IDLE_TTL + Duration::from_secs(1))
        .expect("monotonic clock younger than IDLE_TTL");
    *backend.last_used.lock().unwrap() = past;
}

#[test]
fn a_workspace_still_loading_is_never_evicted_however_long_it_has_been_idle() {
    let (backend, _rx) = unloaded();
    *backend.analyzer.lock().unwrap() = AnalyzerState::Loading;
    age(&backend);

    assert!(!backend.evict_if_idle());
    assert!(matches!(
        *backend.analyzer.lock().unwrap(),
        AnalyzerState::Loading
    ));
}

#[test]
fn a_failed_load_is_left_alone_for_ensure_loading_to_retry() {
    let (backend, _rx) = unloaded();
    *backend.analyzer.lock().unwrap() = AnalyzerState::Failed;
    age(&backend);

    assert!(!backend.evict_if_idle());
    assert!(matches!(
        *backend.analyzer.lock().unwrap(),
        AnalyzerState::Failed
    ));
}

#[test]
fn a_workspace_that_never_loaded_reports_nothing_evicted() {
    let (backend, _rx) = unloaded();
    age(&backend);

    assert!(!backend.evict_if_idle());
}

#[test]
fn touching_clears_a_deadline_that_had_already_come_due() {
    let (backend, _rx) = unloaded();
    age(&backend);
    assert!(backend.last_used.lock().unwrap().elapsed() >= IDLE_TTL);

    backend.touch();

    assert!(backend.last_used.lock().unwrap().elapsed() < IDLE_TTL);
}
