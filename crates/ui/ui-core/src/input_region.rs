use std::cell::RefCell;
use std::collections::HashMap;

use geometry_core::Rect;
use layout_core::NodeId;
use reactive_core::ReadSignal;

thread_local! {
    static INTERACTIVE: RefCell<HashMap<NodeId, ReadSignal<Rect>>> = RefCell::new(HashMap::new());
}

/// Registers `node` as an interactive (press/drag) target, tracking its laid-out `rect` signal. A surface that
/// carves its input region from its content — a click-through overlay such as the notification popups — reads
/// [`interactive_rects`] to receive pointer input only where widgets actually respond. Idempotent per node.
pub fn register_interactive(node: NodeId, rect: ReadSignal<Rect>) {
    INTERACTIVE.with(|m| {
        m.borrow_mut().insert(node, rect);
    });
}

/// Drops `node` from the interactive set — called when its widget is dropped, so a dismissed card stops
/// contributing to the input region.
pub fn unregister_interactive(node: NodeId) {
    INTERACTIVE.with(|m| {
        m.borrow_mut().remove(&node);
    });
}

/// The current laid-out rects of every interactive widget on this surface's thread, dropping any not yet laid
/// out (zero-sized). Read without subscribing (`peek`), so the platform's frame loop can call it outside a
/// reactive scope without accidentally tracking the layout signals.
pub fn interactive_rects() -> Vec<Rect> {
    INTERACTIVE.with(|m| {
        m.borrow()
            .values()
            .map(ReadSignal::peek)
            .filter(|r| r.width > 0.0 && r.height > 0.0)
            .collect()
    })
}
