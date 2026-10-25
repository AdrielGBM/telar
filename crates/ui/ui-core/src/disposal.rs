//! Retiring a child once nothing is running inside it.
//!
//! A container dispatches to a *snapshot* of its children and releases the borrow first, because a handler is allowed to remove the row it is running in — a row with a delete button, a menu item that closes its own menu. [`ReactiveList::on_event`](crate::ReactiveList) spells the rule out: the removed child finishes the event it is in the middle of instead of being dropped underneath itself.
//!
//! That guarantee used to be the handle refcount's. The snapshot's `Rc` kept the widget alive, and the widget's handles kept its signals alive with it. An owner disposed the instant the reconcile drops the row takes those signals away mid-handler, and the next read is a panic rather than a stale value — which is the right failure, and still a failure.
//!
//! So disposal waits for the dispatch to unwind. The invariant is not about lists or events: **nothing is freed while something is executing inside it.**

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use layout_core::NodeId;
use reactive_core::{
    OwnerId, PanicPayload, SurfaceHandle, current_surface, dispose_owner, owner_scope,
};

use crate::context::{layout_generation, record_nodes, remove_node};

reactive_core::dropless_thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    static RETIRED: released Vec<Retired> = const { Vec::new() };
}

struct Retired {
    owner: Option<OwnerId>,
    nodes: Vec<NodeId>,
    surface: SurfaceHandle,
    generation: u64,
}

/// Frees a child's owner and its layout node, once nothing is executing inside it.
pub(crate) fn retire(owner: Option<OwnerId>, node: NodeId) {
    schedule(Retired {
        owner,
        nodes: vec![node],
        surface: current_surface(),
        generation: layout_generation(),
    });
}

/// What a build made under an owner of its own — that owner, and the layout nodes the build created — freed together when this drops, as a retired child is. Disposing the owner frees only the reactive half: a layout node is freed by [`remove_node`] and nothing else, so a tree torn down by its owner alone leaves its nodes laid out with their rect signals freed under them, and the next pass over them panics.
#[must_use = "dropping the build frees what it made"]
pub struct OwnedBuild {
    owner: OwnerId,
    roots: Vec<NodeId>,
    surface: SurfaceHandle,
    generation: u64,
}

impl OwnedBuild {
    /// Runs `build` under a fresh owner, recording the layout nodes it creates.
    pub fn run<T>(build: impl FnOnce() -> T) -> (T, Self) {
        let scope = owner_scope();
        let recording = record_nodes();
        let built = build();
        let roots = recording.keep_roots();
        let owner = scope.id();
        drop(scope);
        let owned = Self {
            owner,
            roots,
            surface: current_surface(),
            generation: layout_generation(),
        };
        (built, owned)
    }
}

impl Drop for OwnedBuild {
    fn drop(&mut self) {
        schedule(Retired {
            owner: Some(self.owner),
            nodes: std::mem::take(&mut self.roots),
            surface: self.surface,
            generation: self.generation,
        });
    }
}

fn schedule(retired: Retired) {
    if DEPTH.with(Cell::get) == 0 {
        let mut panicked = None;
        free(retired, &mut panicked);
        report(panicked);
        return;
    }
    RETIRED.with(|r| r.borrow_mut().push(retired));
}

/// Marks a dispatch walk as running. The outermost guard frees what the walk retired.
///
/// Nesting is the normal case — every container on the path from the event's entry point to the widget that takes it holds one — and only the outermost unwinding means the stack is clear.
#[must_use = "the walk is only marked as running while this guard is alive"]
pub(crate) fn dispatching() -> DispatchGuard {
    DEPTH.with(|depth| depth.set(depth.get() + 1));
    DispatchGuard
}

pub(crate) struct DispatchGuard;

impl Drop for DispatchGuard {
    fn drop(&mut self) {
        let outermost = DEPTH.with(|depth| {
            let left = depth.get().saturating_sub(1);
            depth.set(left);
            left == 0
        });
        if !outermost {
            return;
        }
        let mut panicked = None;
        // Taken rather than drained in place: freeing an owner runs teardown that may retire something else.
        loop {
            let batch = RETIRED.with(|r| std::mem::take(&mut *r.borrow_mut()));
            if batch.is_empty() {
                break;
            }
            for retired in batch {
                free(retired, &mut panicked);
            }
        }
        report(panicked);
    }
}

/// Frees one retired child, containing a panic so the rest of a batch is still freed: a teardown that stops early leaks everything after it.
fn free(retired: Retired, panicked: &mut Option<PanicPayload>) {
    let Retired {
        owner,
        nodes,
        surface,
        generation,
    } = retired;
    // Owner before node: a node freed first has its id back in circulation while the owner still points at it, which lands the cascade on whatever the id names next.
    if let Some(owner) = owner {
        surface.with_entered(|| contain(panicked, || dispose_owner(owner)));
    }
    // A node id names a node only in its own surface's tree, a batch may be freed on another surface's turn, and a surface torn down first or a reset took its tree with it.
    surface.with_entered_if_alive(|| {
        if layout_generation() == generation {
            for node in nodes {
                contain(panicked, || remove_node(node));
            }
        }
    });
}

fn contain(first: &mut Option<PanicPayload>, f: impl FnOnce()) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(f)) {
        first.get_or_insert(payload);
    }
}

/// Resumes the first panic a teardown contained, unless one is already unwinding through here: a second panic out of a `Drop` aborts the process.
fn report(panicked: Option<PanicPayload>) {
    if let Some(payload) = panicked
        && !std::thread::panicking()
    {
        resume_unwind(payload);
    }
}

#[cfg(test)]
#[path = "disposal_test.rs"]
mod tests;
