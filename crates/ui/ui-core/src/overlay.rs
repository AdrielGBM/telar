use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::RwSignal;
use ui_tree::{
    Component, EventResult, OverlaySink, RenderNode, register_overlay, unregister_overlay,
};

use crate::context::{attach_overlay, detach_overlay, remove_node};
use crate::layout_item::{LayoutItem, TrackedChildren, register_container};
use crate::pointer::{dispatch_container_event, offset_pointer};
use crate::scroll_region::visible_rect;

/// Where an anchored overlay's content sits relative to its trigger widget. Maps to the `.rsx` `placement`
/// attribute. Only vertical placements are provided today; horizontal ones would follow the same pattern.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    /// Content's top-left at the trigger's bottom-left — a menu dropping down from its button.
    Below,
    /// Content's bottom-left at the trigger's top-left — a menu opening upward.
    Above,
    /// Beside the trigger on its leading side, centred on it — where a tooltip goes when the trigger sits in
    /// a vertical rail and the room is sideways.
    Start,
    /// Beside the trigger on its trailing side, centred on it.
    End,
}

/// The world-vs-local anchor fallback shared by the anchored menu/select/tooltip panels.
///
/// Uses the trigger's *on-screen* rect, not its laid-out one: a trigger inside a scrolled viewport is drawn
/// somewhere other than where it was laid out, and a panel placed at the laid-out spot lands off by the
/// scroll offset.
pub fn anchor_rect(node: NodeId, fallback: &RwSignal<Rect>) -> Rect {
    visible_rect(node).unwrap_or_else(|| fallback.peek())
}

/// Anchors an overlay's content to a trigger widget. `trigger` is the trigger's laid-out rect (what
/// `track_layout` returns); reading it in `view()` makes the content follow the trigger across relayouts.
#[derive(Clone)]
struct Anchor {
    trigger: RwSignal<Rect>,
    placement: Placement,
}

/// The panel box: the union of the children's laid-out rects (their intrinsic size before anchoring). `read`
/// is `peek` during event routing (untracked) and `get` inside `view()` (so the render follows layout).
fn panel_rect(children: &TrackedChildren, read: impl Fn(&RwSignal<Rect>) -> Rect) -> Rect {
    let mut acc: Option<Rect> = None;
    for child in children {
        if let Some(sig) = &child.rect {
            let r = read(sig);
            acc = Some(acc.map_or(r, |u| u.union(r)));
        }
    }
    acc.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0))
}

/// Gap kept between an anchored panel and the edge it was pushed off, so a shifted bubble does not sit flush
/// against the window.
const EDGE_MARGIN: f32 = 4.0;

/// Gap kept between an anchored panel and the thing it is anchored to.
///
/// A panel flush against its trigger reads as *part of* the trigger, which is exactly what it is not: it is
/// a separate surface that appeared because of it. The gap is what makes a menu look attached to its button
/// rather than grown out of it — 4px is enough to separate the two surfaces and small enough that they still
/// read as one gesture, which is why it is the offset most anchored-panel libraries settle on.
const ANCHOR_GAP: f32 = 4.0;

/// The area an anchored panel has to stay inside. Falls back to an unbounded box before the host has been
/// laid out (the very first frame), where clamping to nothing is the same as not clamping.
fn anchor_viewport() -> Rect {
    crate::context::overlay_viewport().unwrap_or(Rect::new(0.0, 0.0, f32::MAX, f32::MAX))
}

/// The translate that moves `panel` from where it was laid out (near the host origin) to its anchored spot
/// next to `trigger`. Placement picks the target top-left; the offset is that target minus the panel origin.
///
/// Then the panel is kept on screen, which is the half that was missing: **flip** to the other side of the
/// trigger when the asked-for one does not fit and the opposite does, and **shift** along the trigger's edge
/// when it overflows sideways. Without it the placement is read off the trigger alone, so a tooltip on the
/// rightmost button of a toolbar runs past the window and the text wraps into a column — which is not a rare
/// case but the common one. The trigger is never covered: a flip moves the panel to its other side, and a
/// shift only slides along the edge it is already on.
fn anchor_translate(
    trigger: Rect,
    panel: Rect,
    placement: Placement,
    viewport: Rect,
) -> (f32, f32) {
    let fits_below =
        trigger.y + trigger.height + ANCHOR_GAP + panel.height <= viewport.y + viewport.height;
    let fits_above = trigger.y - ANCHOR_GAP - panel.height >= viewport.y;
    let fits_after =
        trigger.x + trigger.width + ANCHOR_GAP + panel.width <= viewport.x + viewport.width;
    let fits_before = trigger.x - ANCHOR_GAP - panel.width >= viewport.x;
    let placement = match placement {
        Placement::Below if !fits_below && fits_above => Placement::Above,
        Placement::Above if !fits_above && fits_below => Placement::Below,
        Placement::Start if !fits_before && fits_after => Placement::End,
        Placement::End if !fits_after && fits_before => Placement::Start,
        other => other,
    };
    // Sideways placement centres on the trigger; the vertical ones align to its leading edge. LTR throughout,
    // as `Below` has always been — flipping the whole function for RTL is one place to change.
    let centre_y = trigger.y + (trigger.height - panel.height) / 2.0;
    let (target_x, target_y) = match placement {
        Placement::Below => (trigger.x, trigger.y + trigger.height + ANCHOR_GAP),
        Placement::Above => (trigger.x, trigger.y - panel.height - ANCHOR_GAP),
        Placement::Start => (trigger.x - panel.width - ANCHOR_GAP, centre_y),
        Placement::End => (trigger.x + trigger.width + ANCHOR_GAP, centre_y),
    };
    // Slide along the edge rather than clamping blindly: a panel wider than the viewport keeps its left edge
    // visible, which is where its content starts.
    let max_x = viewport.x + viewport.width - panel.width - EDGE_MARGIN;
    let target_x = target_x.min(max_x).max(viewport.x + EDGE_MARGIN);
    let max_y = viewport.y + viewport.height - panel.height - EDGE_MARGIN;
    let target_y = target_y.min(max_y.max(viewport.y)).max(viewport.y);
    // On the pixel grid. A sideways placement centres on the trigger, so it lands on a half pixel whenever
    // the panel and the trigger differ by an odd height — and a surface at a half pixel has soft edges and
    // softer text inside it. Rounding a translate cannot move anything anywhere it should not be.
    ((target_x - panel.x).round(), (target_y - panel.y).round())
}

/// The content rect an anchored overlay actually occupies on screen: its panel translated to the trigger.
/// This is the hit-test barrier the registry sees, so only the visible panel blocks — clicks elsewhere fall
/// through even though the underlying content node fills the viewport.
/// Where the anchored panel ends up, and the translate that put it there.
///
/// Both answers come from one derivation — the panel union, then the flip/shift against the viewport — and
/// both are wanted for the same pointer event: the registry hit-tests against the rect, and the dispatcher
/// maps the event back into the children's space by the translate. Derived separately, the two could disagree
/// about where the panel is while agreeing that the pointer was over it.
fn anchored_placement(
    children: &TrackedChildren,
    anchor: &Anchor,
    read: impl Fn(&RwSignal<Rect>) -> Rect,
) -> (Rect, (f32, f32)) {
    let panel = panel_rect(children, &read);
    let (dx, dy) = anchor_translate(
        read(&anchor.trigger),
        panel,
        anchor.placement,
        anchor_viewport(),
    );
    (
        Rect::new(panel.x + dx, panel.y + dy, panel.width, panel.height),
        (dx, dy),
    )
}

fn anchored_content_rect(
    children: &TrackedChildren,
    anchor: &Anchor,
    read: impl Fn(&RwSignal<Rect>) -> Rect,
) -> Rect {
    anchored_placement(children, anchor, read).0
}

/// The overlay's hook into priority pointer routing. Shares the same `Rc<RefCell>` child handles as the
/// `Overlay` widget (`Child` is a cheap clonable handle), so a pointer event dispatched through the sink
/// reaches the very same content the widget renders. `content_rect` is the content container's layout rect,
/// used as the hit-test barrier (a full-viewport scrim blocks everything; an anchored panel only itself).
struct OverlaySinkImpl {
    content_rect: RwSignal<Rect>,
    children: RefCell<TrackedChildren>,
    // Modal (swallow every event over the barrier) vs click-through (only where a child handled it).
    blocking: bool,
    // When set, the barrier and dispatch coordinates track the trigger instead of the fill container.
    anchor: Option<Anchor>,
    // A kept-mounted overlay whose `visible` reads false is inert: an empty barrier so it blocks nothing.
    visible: Rc<dyn Fn() -> bool>,
}

impl OverlaySink for OverlaySinkImpl {
    fn content_rect(&self) -> Rect {
        // Hidden (kept mounted for a modal that toggles visibility): report an empty barrier so no pointer
        // event routes to it and nothing behind is blocked.
        if !(self.visible)() {
            return Rect::default();
        }
        // peek, not get: routing runs during (batched) event dispatch, not inside a tracking effect.
        match &self.anchor {
            None => self.content_rect.peek(),
            Some(anchor) => anchored_content_rect(&self.children.borrow(), anchor, |s| s.peek()),
        }
    }

    fn dispatch(&self, event: &Event) -> EventResult {
        // Anchored content is laid out at its intrinsic (un-anchored) origin but hit at the anchored spot,
        // so map the world event back into the children's local space by the inverse translate first.
        let offset = self
            .anchor
            .as_ref()
            .map(|anchor| anchored_placement(&self.children.borrow(), anchor, |s| s.peek()).1);
        match offset {
            Some((dx, dy)) => {
                // Map world → children-local space: local = world − translate. `offset_pointer(dx,dy)`
                // applies the inverse of translate(dx,dy), i.e. subtracts it — so the sign is POSITIVE
                // (matches scroll_area's use). Negating it double-adds the anchor offset and mishits rows.
                let local = offset_pointer(event, dx as f64, dy as f64);
                let event = local.as_ref().unwrap_or(event);
                dispatch_container_event(&mut self.children.borrow_mut(), event)
            }
            None => dispatch_container_event(&mut self.children.borrow_mut(), event),
        }
    }

    fn blocking(&self) -> bool {
        self.blocking
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
///
/// Variants (all portal the same way, they differ in how they route clicks and where the content sits):
/// - [`Overlay::new`] — modal: blocks every click inside its content rect (a full-viewport scrim).
/// - [`Overlay::anchored_click_through`] — positions the content next to a trigger widget (dropdowns, menus,
///   tooltips) and takes no pointer, so clicks anywhere fall through to the tree behind.
pub struct Overlay {
    // Node handed to the DOM parent: a 0×0 placeholder (when portaled) or the content itself (fallback).
    layout_node: NodeId,
    // The viewport-filling content node; `Some` and attached to the host only when portaled.
    portaled_content: Option<NodeId>,
    children: TrackedChildren,
    // Registry id for priority pointer routing; removed on drop.
    overlay_id: u64,
    // Focus-scope registration, withdrawn on drop alongside the pointer one.
    focus_scope: crate::focus::ScopeId,
    // Set for `anchored`: translates the rendered content to the trigger's rect (see `view`).
    anchor: Option<Anchor>,
    // Read each `view()`: when false the overlay draws nothing (kept mounted so its content — e.g. a modal's
    // slotted body — survives a close/reopen instead of being rebuilt from a consumed slot).
    visible: Rc<dyn Fn() -> bool>,
}

impl Overlay {
    /// A modal portal: the content fills the viewport and blocks every click behind it.
    pub fn new(
        layout_style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        Self::build(layout_style, children, true, None, Rc::new(|| true))
    }

    /// A modal portal that is kept mounted and shown/hidden by `visible` (read each frame). Unlike disposing
    /// and rebuilding the overlay on every open, this preserves its content across close/reopen — needed for a
    /// dialog whose body arrives as a pre-built slot (which cannot be rebuilt once consumed). Hidden, it draws
    /// nothing and blocks nothing.
    pub fn toggleable(
        layout_style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
        visible: impl Fn() -> bool + 'static,
    ) -> Result<Self, LayoutError> {
        Self::build(layout_style, children, true, None, Rc::new(visible))
    }

    /// A portal whose content is positioned next to `trigger` (a dropdown/menu/tooltip popping up by its
    /// button) and takes no pointer: a tooltip bubble, a hint, anything that appears because the pointer is
    /// *near* it and would be dismissed by touching it. The content sizes to its intrinsic panel and is
    /// translated to the trigger's rect per `placement`.
    pub fn anchored_click_through(
        layout_style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
        trigger: RwSignal<Rect>,
        placement: Placement,
    ) -> Result<Self, LayoutError> {
        Self::build(
            layout_style,
            children,
            false,
            Some(Anchor { trigger, placement }),
            Rc::new(|| true),
        )
    }

    fn build(
        layout_style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
        blocking: bool,
        anchor: Option<Anchor>,
        visible: Rc<dyn Fn() -> bool>,
    ) -> Result<Self, LayoutError> {
        // `absolute_fill` takes the layer out of flow and sizes it to its container; attaching it to the
        // host makes that container the viewport. The caller's flex alignment positions content inside; an
        // anchored overlay instead lets its content size intrinsically and moves it with a transform.
        let (content, content_rect, children) =
            register_container(layout_style.absolute_fill(), children)?;

        // Register for priority pointer routing. The sink shares the same child handles as the widget.
        let sink: Rc<dyn OverlaySink> = Rc::new(OverlaySinkImpl {
            content_rect,
            children: RefCell::new(children.clone()),
            blocking,
            anchor: anchor.clone(),
            visible: visible.clone(),
        });
        let overlay_id = register_overlay(sink);
        // The keyboard's half of the same barrier, named by node rather than by the ids inside it: the children were built before this overlay existed, so ancestry has to answer at the moment Tab is pressed.
        let focus_scope = crate::focus::register_scope(
            content,
            {
                let visible = visible.clone();
                move || visible()
            },
            blocking,
        );

        if attach_overlay(content) {
            // Portaled: the DOM parent gets a 0×0 placeholder so the portal takes no space in the flow.
            let (placeholder, _r) =
                crate::context::new_leaf(LayoutStyle::new().width(0.0).height(0.0))?;
            Ok(Overlay {
                layout_node: placeholder,
                portaled_content: Some(content),
                children,
                overlay_id,
                focus_scope,
                anchor,
                visible,
            })
        } else {
            // No host yet: lay the content out in place (it will cover its parent, not the viewport).
            Ok(Overlay {
                layout_node: content,
                portaled_content: None,
                children,
                overlay_id,
                focus_scope,
                anchor,
                visible,
            })
        }
    }
}

impl Overlay {
    /// The node its content actually hangs from, which is the portaled one when it has a host and the in-tree
    /// node before that. What a caller asks for to reason about the content by ancestry — autofocusing what is
    /// inside it, say — since [`layout_node`](LayoutItem::layout_node) is a 0×0 placeholder once portaled.
    pub fn content_node(&self) -> NodeId {
        self.portaled_content.unwrap_or(self.layout_node)
    }
}

impl LayoutItem for Overlay {
    fn layout_node(&self) -> NodeId {
        self.layout_node
    }

    /// An overlay is reached through the registry, before the tree walk, so its in-tree node must not
    /// hit-test at all. Normally it is a 0×0 placeholder and the question never comes up; on the first frame,
    /// before a host exists, the content is laid out in place and would otherwise cover its own siblings.
    fn pointer_opaque(&self) -> bool {
        false
    }
}

impl Component for Overlay {
    fn view(&self) -> RenderNode {
        // Kept mounted but hidden: draw nothing (its content stays alive for the next time it is shown).
        if !(self.visible)() {
            return RenderNode::Empty;
        }
        let boundaries = self.children.iter().map(|c| c.segment.boundary());
        match &self.anchor {
            None => RenderNode::overlay(boundaries),
            Some(anchor) => {
                // `get` (not peek) so the transform re-runs when the trigger or the panel's size changes.
                let panel = panel_rect(&self.children, |s| s.get());
                let (dx, dy) = anchor_translate(
                    anchor.trigger.get(),
                    panel,
                    anchor.placement,
                    anchor_viewport(),
                );
                // Translate matrix `[1,0,0,1,dx,dy]`: the content is laid out at the origin, drawn at the trigger.
                RenderNode::overlay([RenderNode::transform_with(
                    [1.0, 0.0, 0.0, 1.0, dx, dy],
                    boundaries,
                )])
            }
        }
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
        // `view` draws nothing while hidden and `content_rect` is an empty barrier; this was the one path that stayed open, so a shut dialog's field still received every key press.
        // The settling events keep passing, or content hidden mid-gesture holds a hover with no event able to reach it and clear one.
        if !(self.visible)() && !matches!(event, Event::CursorLeft | Event::FocusChanged { .. }) {
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
        crate::focus::unregister_scope(self.focus_scope);
        // Detach the portaled content from the host and free it when the overlay is disposed (e.g. a
        // reactive `if` hiding a modal) — it lives outside the DOM subtree, so nothing else removes it.
        if let Some(content) = self.portaled_content {
            detach_overlay(content);
            remove_node(content);
        }
    }
}

#[cfg(test)]
impl Overlay {
    // The on-screen content rect (the hit-test barrier the registry sees) for an anchored overlay.
    fn anchored_barrier(&self) -> Rect {
        let anchor = self.anchor.as_ref().expect("overlay is not anchored");
        anchored_content_rect(&self.children, anchor, |s| s.peek())
    }
}

#[cfg(test)]
mod tests {
    use crate::context::reset_layout_runtime;
    use layout_core::AvailableSpace;
    use platform_core::{PointerButton, PointerSource};
    use reactive_core::{RwSignal, signal};

    use super::*;
    use crate::ComponentList;

    /// The other half of the same barrier, and what §1.1 of the audit actually asked for: while a modal is up,
    /// Tab must not walk out to the content behind the scrim. The pointer has been blocked there all along;
    /// the keyboard was not, because the tab order is a list and a list has no notion of in front or behind.
    #[test]
    fn a_modal_that_is_up_holds_tab_inside_itself() {
        use crate::{StyledContainer, focus};

        reset_layout_runtime();
        let behind = focus::next_id();
        focus::register_as(behind, focus::FocusKind::Widget);

        let below = focus::next_id();
        let inside = StyledContainer::new(
            LayoutStyle::new().width(50.0).height(20.0),
            |_r| renderer_core::RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_focus(|_| {});
        let _overlay =
            Overlay::toggleable(LayoutStyle::new(), vec![Box::new(inside)], || true).unwrap();
        let above = focus::next_id();

        focus::request(behind);
        focus::focus_next();
        let landed = focus::current().expect("something took focus");
        assert!(
            landed > below && landed < above,
            "Tab left the modal that is up and landed on the content behind it"
        );
    }

    /// `view` draws nothing while hidden and `content_rect` is an empty barrier, but the in-tree walk stayed
    /// open — so every key press still reached the children of a dialog that was shut. The settling events
    /// are the exception, or content hidden mid-gesture keeps a hover it has no way left to clear.
    #[test]
    fn a_hidden_overlay_does_not_take_the_keyboard() {
        use std::cell::Cell;
        use std::rc::Rc;

        reset_layout_runtime();
        let keys = Rc::new(Cell::new(0u32));
        let counted = keys.clone();
        let showing = signal(false);
        let flag = showing.clone();
        let field = crate::StyledContainer::new(
            LayoutStyle::new().width(50.0).height(20.0),
            |_r| renderer_core::RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_key(move |_| counted.set(counted.get() + 1));
        let mut overlay =
            Overlay::toggleable(LayoutStyle::new(), vec![Box::new(field)], move || {
                flag.get()
            })
            .unwrap();

        let press = Event::KeyPressed {
            key: platform_core::Key::Char('a'),
            modifiers: platform_core::ModifiersState::default(),
        };
        overlay.on_event(&press);
        assert_eq!(keys.get(), 0, "a shut dialog takes no keys");

        showing.set(true);
        overlay.on_event(&press);
        assert_eq!(keys.get(), 1, "and takes them again once it is up");

        // Hidden mid-gesture: the settling events still get through, or the content keeps the look it had with nothing able to reach it.
        showing.set(false);
        assert_eq!(
            overlay.on_event(&Event::CursorLeft),
            crate::EventResult::Ignored,
            "CursorLeft reaches the children (they simply had nothing to settle)"
        );
    }

    /// The pointer path already scopes itself to what is on screen: a kept-mounted overlay whose `visible`
    /// reads false is inert, an empty barrier that blocks nothing. The keyboard path does not, and the two
    /// disagreeing is the bug — a focusable joins the tab order when its widget is *built*, and `toggleable`
    /// builds its subtree once and keeps it mounted, so a field inside a dialog that is shut is still a Tab
    /// stop. Not merely reachable past a scrim, as first described: reachable when nothing is open at all.
    ///
    /// Closed by naming a *node* rather than a set of ids: the overlay's children are constructed before the
    /// overlay that will host them, so it never learns which focusables are its own, and ancestry answers at
    /// the moment Tab is pressed instead.
    #[test]
    fn tab_does_not_walk_into_an_overlay_that_is_not_showing() {
        use crate::{StyledContainer, focus};

        reset_layout_runtime();
        let base = focus::next_id();
        focus::register_as(base, focus::FocusKind::Widget);

        // Ids allocated between the two markers belong to whatever the overlay built.
        let below = focus::next_id();
        let field = StyledContainer::new(
            LayoutStyle::new().width(50.0).height(20.0),
            |_r| renderer_core::RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_focus(|_| {});
        let _overlay =
            Overlay::toggleable(LayoutStyle::new(), vec![Box::new(field)], || false).unwrap();
        let above = focus::next_id();

        focus::request(base);
        focus::focus_next();
        let landed = focus::current().expect("something took focus");
        assert!(
            !(landed > below && landed < above),
            "Tab reached a focusable inside an overlay that is not showing"
        );
    }

    /// A panel is placed from its trigger and then kept on screen. Without the second half, a tooltip on the
    /// rightmost button of a toolbar is laid out past the window edge and its text wraps into a column — the
    /// shape of the bug, not a cosmetic offset.
    #[test]
    fn an_anchored_panel_shifts_and_flips_to_stay_on_screen() {
        let viewport = Rect::new(0.0, 0.0, 400.0, 300.0);
        let panel = Rect::new(0.0, 0.0, 120.0, 60.0);

        // Comfortably inside: under the trigger, a gap short of touching it.
        let trigger = Rect::new(100.0, 100.0, 40.0, 20.0);
        assert_eq!(
            anchor_translate(trigger, panel, Placement::Below, viewport),
            (100.0, 120.0 + ANCHOR_GAP)
        );

        // Against the right edge: slid left just enough to fit, still below the trigger.
        let right = Rect::new(380.0, 100.0, 20.0, 20.0);
        let (dx, dy) = anchor_translate(right, panel, Placement::Below, viewport);
        assert_eq!((dx, dy), (400.0 - 120.0 - EDGE_MARGIN, 120.0 + ANCHOR_GAP));

        // Against the bottom edge, with room above: flipped over the trigger rather than clamped onto it.
        let low = Rect::new(100.0, 270.0, 40.0, 20.0);
        let (_, dy) = anchor_translate(low, panel, Placement::Below, viewport);
        assert_eq!(dy, 270.0 - 60.0 - ANCHOR_GAP, "opens upward instead");

        // Nowhere to flip to (a panel taller than the viewport): pinned to the top, so its start is visible.
        let tall = Rect::new(0.0, 0.0, 120.0, 400.0);
        let (_, dy) = anchor_translate(low, tall, Placement::Below, viewport);
        assert_eq!(dy, 0.0);
    }

    /// Beside the trigger, the panel centres on it and flips across it when its own side runs out — the same
    /// two rules the vertical placements follow, on the other axis. A control in a vertical rail has no room
    /// below it and all the room in the world beside it, which is why the sideways pair exists at all.
    #[test]
    fn a_panel_placed_beside_its_trigger_centres_on_it_and_flips_when_it_has_to() {
        let viewport = Rect::new(0.0, 0.0, 400.0, 300.0);
        let panel = Rect::new(0.0, 0.0, 120.0, 60.0);
        let trigger = Rect::new(200.0, 100.0, 40.0, 20.0);

        let (dx, dy) = anchor_translate(trigger, panel, Placement::End, viewport);
        assert_eq!(
            dx,
            240.0 + ANCHOR_GAP,
            "starts a gap past where the trigger ends"
        );
        assert_eq!(dy, 100.0 + (20.0 - 60.0) / 2.0, "centred on the trigger");

        let (dx, _) = anchor_translate(trigger, panel, Placement::Start, viewport);
        assert_eq!(
            dx,
            80.0 - ANCHOR_GAP,
            "ends a gap before the trigger starts"
        );

        // A rail down the left edge: there is no room before the trigger, so the panel takes the other side.
        let rail = Rect::new(4.0, 100.0, 40.0, 20.0);
        let (dx, _) = anchor_translate(rail, panel, Placement::Start, viewport);
        assert_eq!(dx, 44.0 + ANCHOR_GAP, "flipped to the trailing side");

        // Centring on a trigger of a different height lands on a half pixel, and a surface at a half pixel
        // has soft edges and softer text: a tooltip beside a 36px button was crisp and the same one under a
        // 28px tab was blurred, from nothing but the fraction each placement contributed.
        let odd = Rect::new(200.0, 100.0, 40.0, 21.0);
        let (dx, dy) = anchor_translate(odd, panel, Placement::End, viewport);
        assert_eq!((dx, dy), (dx.round(), dy.round()), "on the pixel grid");
    }
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
    fn pressable(flag: RwSignal<bool>) -> Container {
        Container::new(LayoutStyle::new().width(400.0).height(400.0), vec![])
            .unwrap()
            .on_press(move || flag.set(true))
    }

    // Baseline (guards the assertion below from being vacuous): with no overlay, a tap on the background
    // fires its on_press.
    #[test]
    fn background_alone_receives_tap() {
        reset_layout_runtime();
        let clicked = signal(false);
        let bg = pressable(clicked.clone());
        let root = Container::new(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            vec![Box::new(bg)],
        )
        .unwrap();
        let root_node = root.layout_node();
        compute_layout(
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
        reset_layout_runtime();
        let bg_clicked = signal(false);
        let overlay_clicked = signal(false);

        let bg = pressable(bg_clicked.clone());
        // The scrim fills the overlay (which `absolute_fill`s the root), so it covers the background.
        let scrim = Container::new(LayoutStyle::new().width(400.0).height(400.0), vec![])
            .unwrap()
            .on_press({
                let s = overlay_clicked.clone();
                move || s.set(true)
            });
        let overlay = Overlay::new(LayoutStyle::new(), vec![Box::new(scrim)]).unwrap();
        let root = Container::new(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            vec![Box::new(bg), Box::new(overlay)],
        )
        .unwrap();
        let root_node = root.layout_node();
        compute_layout(
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

        reset_layout_runtime();
        let bg_clicked = signal(false);

        // 1. Lay out the page first: this registers `root` as the overlay host.
        let bg = pressable(bg_clicked.clone());
        let root = Container::new(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            vec![Box::new(bg)],
        )
        .unwrap();
        let root_node = root.layout_node();
        compute_layout(
            root_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(root);
        let _ = tree.commands();

        // 2. Now open the modal: its content portals to the host and fills the viewport after relayout.
        let overlay_clicked = signal(false);
        let scrim = Container::new(LayoutStyle::new().width(400.0).height(400.0), vec![])
            .unwrap()
            .on_press({
                let s = overlay_clicked.clone();
                move || s.set(true)
            });
        let _overlay = Overlay::new(LayoutStyle::new(), vec![Box::new(scrim)]).unwrap();
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

    // Deliverable 1 at the widget level: a click-through overlay with a small panel lets a tap on its
    // transparent area reach the background, but still consumes a tap that lands on the panel.
    #[test]
    fn click_through_overlay_lets_background_tap_through() {
        reset_layout_runtime();
        let bg_clicked = signal(false);
        let panel_clicked = signal(false);

        let bg = pressable(bg_clicked.clone());
        // A 100×100 panel in the top-left corner; the rest of the click-through layer is transparent.
        let panel = Container::new(LayoutStyle::new().width(100.0).height(100.0), vec![])
            .unwrap()
            .on_press({
                let s = panel_clicked.clone();
                move || s.set(true)
            });
        let overlay = Overlay::build(
            LayoutStyle::new(),
            vec![Box::new(panel)],
            false,
            None,
            Rc::new(|| true),
        )
        .unwrap();
        let root = Container::new(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            vec![Box::new(bg), Box::new(overlay)],
        )
        .unwrap();
        let root_node = root.layout_node();
        compute_layout(
            root_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(root);
        let _ = tree.commands();

        // A tap outside the panel falls through the transparent layer to the background.
        route(&mut tree, &press(200.0, 200.0));
        route(&mut tree, &release(200.0, 200.0));
        assert!(
            bg_clicked.get(),
            "a tap on the transparent area must reach the background"
        );
        assert!(
            !panel_clicked.get(),
            "the panel must not receive a tap outside it"
        );

        // A tap on the panel is consumed by the overlay and does not reach the background.
        bg_clicked.set(false);
        route(&mut tree, &press(50.0, 50.0));
        route(&mut tree, &release(50.0, 50.0));
        assert!(panel_clicked.get(), "a tap on the panel must reach it");
        assert!(
            !bg_clicked.get(),
            "the panel must block the tap from the background"
        );
    }

    // Deliverable 2: an anchored overlay's on-screen content rect origin tracks its trigger rect, and
    // follows the trigger when it moves — proving the content is positioned against the trigger, not the fill.
    #[test]
    fn anchored_content_tracks_trigger() {
        use crate::context::relayout_if_dirty;

        reset_layout_runtime();

        // 1. Lay out a page first so the overlay host exists (the anchored content portals to it).
        let root = Container::new(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            vec![],
        )
        .unwrap();
        let root_node = root.layout_node();
        compute_layout(
            root_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let tree = ComponentList::new(root);
        let _ = tree.commands();

        // 2. Open an anchored overlay below a trigger, with a fixed 120×60 panel.
        let trigger = signal(Rect::new(50.0, 20.0, 80.0, 30.0));
        let panel = Container::new(LayoutStyle::new().width(120.0).height(60.0), vec![]).unwrap();
        let overlay = Overlay::build(
            LayoutStyle::new(),
            vec![Box::new(panel)],
            true,
            Some(Anchor {
                trigger: trigger.clone(),
                placement: Placement::Below,
            }),
            Rc::new(|| true),
        )
        .unwrap();
        relayout_if_dirty();

        // Below: the content sits at the trigger's bottom-left (50, 20 + 30), a gap short of touching it.
        let rect = overlay.anchored_barrier();
        assert_eq!((rect.x, rect.y), (50.0, 50.0 + ANCHOR_GAP));
        assert_eq!((rect.width, rect.height), (120.0, 60.0));

        // Move the trigger; the anchored content origin follows it (no relayout needed — it is a transform).
        trigger.set(Rect::new(200.0, 100.0, 80.0, 30.0));
        let rect = overlay.anchored_barrier();
        assert_eq!((rect.x, rect.y), (200.0, 130.0 + ANCHOR_GAP));
        assert_eq!((rect.width, rect.height), (120.0, 60.0));
    }
}
