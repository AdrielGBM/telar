//! Task work on the browser's event loop.
//!
//! There is one thread, so there is no pool: a job is queued as a microtask and runs when the current turn of the loop yields. That is a real behavioural difference and not a shim — a job that *blocks* freezes the page, where on a native target it would only occupy a worker. Nothing here can fix that, and nothing should pretend to: work that waits on the network or the disk reaches the web through an async transport, and `spawn_task` is where its result is delivered rather than where it waits.

/// `Send` is kept in the signature although nothing here crosses a thread, so that the same job type compiles for both targets and an app cannot accidentally write one that only builds for the web.
pub(in crate::task) type Job = Box<dyn FnOnce() + Send>;

pub(in crate::task) fn submit(job: Job) {
    wasm_bindgen_futures::spawn_local(async move { job() });
}

/// Nothing to stop and nothing to join: a queued microtask is owned by the browser, and there is no thread parked in code a hot reload could unmap.
pub(in crate::task) fn shutdown_and_join() {}
