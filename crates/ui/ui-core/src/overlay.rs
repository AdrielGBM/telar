use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::RwSignal;
use ui_tree::{
    Component, EventResult, OverlaySink, RenderNode, register_overlay, unregister_overlay,
};

use crate::context::{WidgetCtx, attach_overlay, detach_overlay, remove_node};
use crate::layout_item::{LayoutItem, TrackedChildren, register_container};
use crate::pointer::dispatch_container_event;

/// The overlay's hook into priority pointer routing. Shares the same `Rc<RefCell>` child handles as the
/// `Overlay` widget (`Child` is a cheap clonable handle), so a pointer event dispatched through the sink
/// reaches the very same content the widget renders. `content_rect` is the content container's layout rect,
/// used as the hit-test barrier (a full-viewport scrim blocks everything; a small panel only itself).
struct OverlaySinkImpl {
    content_rect: RwSignal<Rect>,
    children: RefCell<TrackedChildren>,
}

impl OverlaySink for OverlaySinkImpl {
    fn content_rect(&self) -> Rect {
        // peek, not get: routing runs during (batched) event dispatch, not inside a tracking effect.
        self.content_rect.peek()
    }

    fn dispatch(&self, event: &Event) -> EventResult {
        dispatch_container_event(&mut self.children.borrow_mut(), event)
    }
}

/// A portal layer: its content is laid out out-of-flow, filling the viewport, and hoisted to the top at
/// compose time — drawn above everything and free of any ancestor clip/transform. A base primitive:
/// unstyled; wrap content in a `box` for a scrim/panel, and position it with normal flex (`align`/`justify`).
///
/// The content is a separate layout node **attached to the layout root** (the overlay host), not to the
/// widget's DOM parent — so a portal declared deep in the tree (e.g. inside a reactive `if`) still covers
/// the whole window instead of collapsing to its parent's box. The widget hands its DOM parent only a
/// zero-size placeholder, so it never affects sibling layout. If no host has been laid out yet (a portal
/// present at the very first frame), it falls back to laying the content out in place.
///
/// Positioned pointer events reach the content with priority via a thread-local overlay registry (see
/// `ui_tree::overlay_dispatch`): a click on the overlay is routed here before the main tree walk and does
/// not fall through to the content behind it, so a scrim that fills the viewport reads as a modal.
pub struct Overlay {
    // Node handed to the DOM parent: a 0×0 placeholder (when portaled) or the content itself (fallback).
    layout_node: NodeId,
    // The viewport-filling content node; `Some` and attached to the host only when portaled.
    portaled_content: Option<NodeId>,
    children: TrackedChildren,
    // Registry id for priority pointer routing; removed on drop.
    overlay_id: u64,
}

impl Overlay {
    pub fn new(
        ctx: &mut WidgetCtx,
        layout_style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        // `absolute_fill` takes the layer out of flow and sizes it to its container; attaching it to the
        // host makes that container the viewport. The caller's flex alignment positions content inside.
        let (content, content_rect, children) =
            register_container(ctx, layout_style.absolute_fill(), children)?;

        // Register for priority pointer routing. The sink shares the same child handles as the widget.
        let sink: Rc<dyn OverlaySink> = Rc::new(OverlaySinkImpl {
            content_rect,
            children: RefCell::new(children.clone()),
        });
        let overlay_id = register_overlay(sink);

        if attach_overlay(content) {
            // Portaled: the DOM parent gets a 0×0 placeholder so the portal takes no space in the flow.
            let (placeholder, _r) =
                crate::context::new_leaf(ctx, LayoutStyle::new().width(0.0).height(0.0))?;
            Ok(Overlay {
                layout_node: placeholder,
                portaled_content: Some(content),
                children,
                overlay_id,
            })
        } else {
            // No host yet: lay the content out in place (it will cover its parent, not the viewport).
            Ok(Overlay {
                layout_node: content,
                portaled_content: None,
                children,
                overlay_id,
            })
        }
    }
}

impl LayoutItem for Overlay {
    fn layout_node(&self) -> NodeId {
        self.layout_node
    }
}

impl Component for Overlay {
    fn view(&self) -> RenderNode {
        RenderNode::overlay(self.children.iter().map(|c| c.segment.boundary()))
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // Positioned pointer events are delivered with priority through the overlay registry (before this
        // in-tree walk reaches us); dispatching them here too would double-fire. Non-positioned events
        // (keyboard shortcuts, CursorLeft) still flow through the tree, so forward those to the content.
        if matches!(
            event,
            Event::PointerPressed { .. }
                | Event::PointerMoved { .. }
                | Event::PointerReleased { .. }
        ) {
            return EventResult::Ignored;
        }
        dispatch_container_event(&mut self.children, event)
    }

    fn debug_name(&self) -> &'static str {
        "Overlay"
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        unregister_overlay(self.overlay_id);
        // Detach the portaled content from the host and free it when the overlay is disposed (e.g. a
        // reactive `if` hiding a modal) — it lives outside the DOM subtree, so nothing else removes it.
        if let Some(content) = self.portaled_content {
            detach_overlay(content);
            remove_node(content);
        }
    }
}

#[cfg(test)]
mod tests {
    use layout_core::AvailableSpace;
    use platform_core::{PointerButton, PointerSource};
    use reactive_core::{RwSignal, signal};

    use super::*;
    use crate::ComponentList;
    use crate::container::Container;
    use crate::context::compute_layout;

    fn press(x: f64, y: f64) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }
    fn release(x: f64, y: f64) -> Event {
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }

    // Mirror the runner: consult the overlay registry first, then walk the tree only if no overlay
    // consumed the event. (Production does this in `handler.rs` via the `App::dispatch_overlays` bridge.)
    fn route(tree: &mut ComponentList, event: &Event) {
        if crate::dispatch_overlays(event) == EventResult::Ignored {
            tree.on_event(event);
        }
    }

    // A container filling 400×400 whose on_press flips `flag`, used as both the modal scrim and the
    // background it covers.
    fn pressable(ctx: &mut WidgetCtx, flag: RwSignal<bool>) -> Container {
        Container::new(ctx, LayoutStyle::new().width(400.0).height(400.0), vec![])
            .unwrap()
            .on_press(move || flag.set(true))
    }

    // Baseline (guards the assertion below from being vacuous): with no overlay, a tap on the background
    // fires its on_press.
    #[test]
    fn background_alone_receives_tap() {
        let mut ctx = WidgetCtx::new();
        let clicked = signal(false);
        let bg = pressable(&mut ctx, clicked.clone());
        let root = Container::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            vec![Box::new(bg)],
        )
        .unwrap();
        let root_node = root.layout_node();
        compute_layout(
            &mut ctx,
            root_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(root);
        let _ = tree.commands();

        route(&mut tree, &press(200.0, 200.0));
        route(&mut tree, &release(200.0, 200.0));
        assert!(
            clicked.get(),
            "background on_press must fire without an overlay"
        );
    }

    // The fix: an overlay is hit-tested before the tree, so a tap over it reaches the overlay's content
    // (the scrim) and is blocked from the background it covers.
    #[test]
    fn overlay_receives_tap_and_blocks_background() {
        let mut ctx = WidgetCtx::new();
        let bg_clicked = signal(false);
        let overlay_clicked = signal(false);

        let bg = pressable(&mut ctx, bg_clicked.clone());
        // The scrim fills the overlay (which `absolute_fill`s the root), so it covers the background.
        let scrim = Container::new(
            &mut ctx,
            LayoutStyle::new().width(400.0).height(400.0),
            vec![],
        )
        .unwrap()
        .on_press({
            let s = overlay_clicked.clone();
            move || s.set(true)
        });
        let overlay = Overlay::new(&mut ctx, LayoutStyle::new(), vec![Box::new(scrim)]).unwrap();
        let root = Container::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            vec![Box::new(bg), Box::new(overlay)],
        )
        .unwrap();
        let root_node = root.layout_node();
        compute_layout(
            &mut ctx,
            root_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(root);
        let _ = tree.commands();

        // A tap at the center hits both the background and the overlay; the overlay must win.
        route(&mut tree, &press(200.0, 200.0));
        route(&mut tree, &release(200.0, 200.0));

        assert!(
            overlay_clicked.get(),
            "the tap must reach the overlay content"
        );
        assert!(
            !bg_clicked.get(),
            "the overlay must block the tap from the content behind it"
        );
    }

    // The real modal scenario: the page is laid out first (registering the overlay host), THEN the modal
    // opens and portals its content to the host (attach_overlay succeeds). This exercises the portaled
    // path — where `content_rect` is driven to the viewport by a later relayout — not the in-place
    // fallback the test above hits (overlay built before any layout host exists).
    #[test]
    fn portaled_overlay_blocks_background() {
        use crate::context::relayout_if_dirty;

        let mut ctx = WidgetCtx::new();
        let bg_clicked = signal(false);

        // 1. Lay out the page first: this registers `root` as the overlay host.
        let bg = pressable(&mut ctx, bg_clicked.clone());
        let root = Container::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            vec![Box::new(bg)],
        )
        .unwrap();
        let root_node = root.layout_node();
        compute_layout(
            &mut ctx,
            root_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(root);
        let _ = tree.commands();

        // 2. Now open the modal: its content portals to the host and fills the viewport after relayout.
        let overlay_clicked = signal(false);
        let scrim = Container::new(
            &mut ctx,
            LayoutStyle::new().width(400.0).height(400.0),
            vec![],
        )
        .unwrap()
        .on_press({
            let s = overlay_clicked.clone();
            move || s.set(true)
        });
        let _overlay = Overlay::new(&mut ctx, LayoutStyle::new(), vec![Box::new(scrim)]).unwrap();
        relayout_if_dirty();

        // 3. A tap at the center must reach the portaled overlay and be blocked from the page behind it.
        route(&mut tree, &press(200.0, 200.0));
        route(&mut tree, &release(200.0, 200.0));

        assert!(
            overlay_clicked.get(),
            "the tap must reach the portaled overlay content"
        );
        assert!(
            !bg_clicked.get(),
            "the portaled overlay must block the tap from the page behind it"
        );
    }
}
