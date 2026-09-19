//! Which box decides the pointer's shape: the innermost one being dragged, else the innermost one hovered, else nobody and the default.

use std::cell::Cell;

use platform_core::{Cursor, WindowCommand};
use reactive_core::SurfaceHandle;

#[derive(Clone, Copy)]
struct Claim {
    id: u64,
    depth: u32,
    cursor: Cursor,
    hovered: bool,
    dragging: bool,
}

struct Cursors {
    claims: Vec<Claim>,
    shown: Cursor,
}

impl Cursors {
    fn new() -> Self {
        Self {
            claims: Vec::new(),
            shown: Cursor::Default,
        }
    }

    fn wanted(&self) -> Cursor {
        let deepest = |pick: fn(&Claim) -> bool| {
            self.claims
                .iter()
                .filter(|claim| pick(claim))
                .max_by_key(|claim| claim.depth)
        };
        deepest(|claim| claim.dragging)
            .or_else(|| deepest(|claim| claim.hovered))
            .map_or(Cursor::Default, |claim| claim.cursor)
    }
}

reactive_core::surface_local! {
    /// Per surface, because each window shows its own pointer shape.
    slot CURSORS: Cursors = Cursors::new();
    access with_cursors, with_cursors_ref;
    context CursorContext, CursorGuard;
}

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
    // Per thread rather than per surface: a claim can outlive its surface, and an id another surface also minted would withdraw a stranger's claim.
    static NEXT_CLAIM: Cell<u64> = const { Cell::new(0) };
}

/// How many boxes deep the event being dispatched is. Only its order along one path matters: a child always reads more than its parent.
pub(crate) fn depth() -> u32 {
    DEPTH.with(Cell::get)
}

/// Runs `dispatch` one level deeper, so the boxes it reaches rank above the one dispatching.
pub(crate) fn nested<R>(dispatch: impl FnOnce() -> R) -> R {
    let _deeper = Deeper::enter();
    dispatch()
}

/// One level of [`nested`], given back even when the dispatch unwinds.
struct Deeper;

impl Deeper {
    fn enter() -> Self {
        DEPTH.with(|d| d.set(d.get() + 1));
        Self
    }
}

impl Drop for Deeper {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// One box's say in the pointer's shape, withdrawn when dropped from the record of the surface it was made on, whichever surface is active then.
pub(crate) struct CursorClaim {
    id: u64,
    surface: SurfaceHandle,
}

impl CursorClaim {
    pub(crate) fn new(cursor: Cursor) -> Self {
        let id = NEXT_CLAIM.with(|next| {
            next.set(next.get() + 1);
            next.get()
        });
        with_cursors(|c| {
            c.claims.push(Claim {
                id,
                depth: 0,
                cursor,
                hovered: false,
                dragging: false,
            })
        });
        Self {
            id,
            surface: reactive_core::current_surface(),
        }
    }

    pub(crate) fn id(&self) -> CursorId {
        CursorId(self.id)
    }

    pub(crate) fn hover(&self, depth: u32, hovered: bool) {
        self.id().edit(|claim| {
            claim.depth = depth;
            claim.hovered = hovered;
        });
    }

    pub(crate) fn drag(&self, depth: u32, dragging: bool) {
        self.id().edit(|claim| {
            if dragging {
                claim.depth = depth;
            }
            claim.dragging = dragging;
        });
    }
}

impl Drop for CursorClaim {
    fn drop(&mut self) {
        let id = self.id;
        let _entered = self.surface.enter();
        // A surface torn down first took its claims with it, and whichever world is active now is not this claim's to publish to.
        if reactive_core::current_surface() != self.surface {
            return;
        }
        with_cursors(|c| c.claims.retain(|claim| claim.id != id));
        publish();
    }
}

/// A handle to a claim that outlives nothing: once the claim is dropped, edits through it do nothing.
#[derive(Clone, Copy)]
pub(crate) struct CursorId(u64);

impl CursorId {
    pub(crate) fn set_shape(self, cursor: Cursor) {
        self.edit(|claim| claim.cursor = cursor);
    }

    fn edit(self, f: impl FnOnce(&mut Claim)) {
        let changed = with_cursors(|c| {
            let Some(claim) = c.claims.iter_mut().find(|claim| claim.id == self.0) else {
                return false;
            };
            let before = *claim;
            f(claim);
            before.hovered != claim.hovered
                || before.dragging != claim.dragging
                || before.cursor != claim.cursor
                || before.depth != claim.depth
        });
        if changed {
            publish();
        }
    }
}

/// Requests the shape the claims now call for, if it is not the one already requested.
fn publish() {
    let changed = with_cursors(|c| {
        let wanted = c.wanted();
        (wanted != c.shown).then(|| {
            c.shown = wanted;
            wanted
        })
    });
    if let Some(cursor) = changed {
        platform_core::push_window_command(WindowCommand::SetCursor(cursor));
    }
}

/// The shape last requested on the active surface.
pub fn requested_cursor() -> Cursor {
    with_cursors_ref(|c| c.shown)
}

pub(crate) fn reset() {
    with_cursors(|c| {
        c.claims.clear();
        c.shown = Cursor::Default;
    });
    DEPTH.with(|d| d.set(0));
}

#[cfg(test)]
#[path = "cursor_test.rs"]
mod tests;
