//! Retiring a child once nothing is running inside it.
//!
//! A container dispatches to a *snapshot* of its children and releases the borrow first, because a handler is
//! allowed to remove the row it is running in — a row with a delete button, a menu item that closes its own
//! menu. [`ReactiveList::on_event`](crate::ReactiveList) spells the rule out: the removed child finishes the
//! event it is in the middle of instead of being dropped underneath itself.
//!
//! That guarantee used to be the handle refcount's. The snapshot's `Rc` kept the widget alive, and the
//! widget's handles kept its signals alive with it. An owner disposed the instant the reconcile drops the row
//! takes those signals away mid-handler, and the next read is a panic rather than a stale value — which is
//! the right failure, and still a failure.
//!
//! So disposal waits for the dispatch to unwind. The invariant is not about lists or events: **nothing is
//! freed while something is executing inside it.**

use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;

use layout_core::NodeId;
use reactive_core::{OwnerId, dispose_owner};

use crate::context::remove_node;

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    // ManuallyDrop for the reason every other TLS slot here carries it: a destructor registered from the app dylib makes dlclose unsafe.
    static RETIRED: ManuallyDrop<RefCell<Vec<Retired>>> =
        ManuallyDrop::new(RefCell::new(Vec::new()));
}

struct Retired {
    owner: Option<OwnerId>,
    node: NodeId,
}

/// Frees a child's owner and its layout node, once nothing is executing inside it.
pub(crate) fn retire(owner: Option<OwnerId>, node: NodeId) {
    let retired = Retired { owner, node };
    if DEPTH.with(Cell::get) == 0 {
        free(retired);
        return;
    }
    RETIRED.with(|r| r.borrow_mut().push(retired));
}

/// Marks a dispatch walk as running. The outermost guard frees what the walk retired.
///
/// Nesting is the normal case — every container on the path from the event's entry point to the widget that
/// takes it holds one — and only the outermost unwinding means the stack is clear.
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
        // Taken rather than drained in place: freeing an owner runs arbitrary teardown, which is allowed to retire something else.
        loop {
            let batch = RETIRED.with(|r| std::mem::take(&mut *r.borrow_mut()));
            if batch.is_empty() {
                return;
            }
            for retired in batch {
                free(retired);
            }
        }
    }
}

fn free(retired: Retired) {
    // Owner before node. A node freed first has its id back in circulation while the owner still holds a declaration pointing at it, which is how the cascade lands on whatever the id names next.
    if let Some(owner) = retired.owner {
        dispose_owner(owner);
    }
    remove_node(retired.node);
}
