//! Where the work behind [`spawn_task`](super::spawn_task) and [`spawn_stream`](super::spawn_stream) runs.
//!
//! Two answers, because the question has two answers. On a target with threads a job goes to a worker and the
//! UI thread carries on; on the web there is one thread and the browser's own event loop, so a job is queued
//! on it. Both honour the same contract — the job runs, and whatever it posts back is drained on the UI
//! thread by [`drain_tasks`](super::drain_tasks) — and neither is visible above this module.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub(super) use native::{shutdown_and_join, submit};
#[cfg(target_arch = "wasm32")]
pub(super) use web::{shutdown_and_join, submit};
