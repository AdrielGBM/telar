use std::collections::HashMap;

use geometry_core::Rect;
use layout_core::NodeId;
use reactive_core::ReadSignal;

reactive_core::surface_local! {
    /// A per-surface set of interactive (press/drag) targets and their laid-out rect signals. The runner
    /// activates each surface's [`InputRegionContext`] around its build/event/frame.
    slot INTERACTIVE: HashMap<NodeId, ReadSignal<Rect>> = HashMap::new();
    access with_interactive, with_interactive_ref;
    context InputRegionContext, InputRegionGuard;
}

/// Registers `node` as an interactive (press/drag) target, tracking its laid-out `rect` signal. A surface that
/// carves its input region from its content — a click-through overlay such as the notification popups — reads
/// [`interactive_rects`] to receive pointer input only where widgets actually respond. Idempotent per node.
pub fn register_interactive(node: NodeId, rect: ReadSignal<Rect>) {
    with_interactive(|m| {
        m.insert(node, rect);
    });
}

/// Drops `node` from the interactive set — called when its widget is dropped, so a dismissed card stops
/// contributing to the input region.
pub fn unregister_interactive(node: NodeId) {
    with_interactive(|m| {
        m.remove(&node);
    });
}

/// The current laid-out rects of every interactive widget on the active surface, dropping any not yet laid
/// out (zero-sized). Read without subscribing (`peek`), so the platform's frame loop can call it outside a
/// reactive scope without accidentally tracking the layout signals.
pub fn interactive_rects() -> Vec<Rect> {
    with_interactive_ref(|m| {
        m.values()
            .map(ReadSignal::peek)
            .filter(|r| r.width > 0.0 && r.height > 0.0)
            .collect()
    })
}
