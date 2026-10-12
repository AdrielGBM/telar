//! [`Overlay`]: a portal layer — content laid out out-of-flow, hoisted to the top at compose time, and hit-tested ahead of the tree.

use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::{Rect, Size};
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::RwSignal;
use ui_tree::{
    Component, EventResult, OverlaySink, RenderNode, register_overlay, unregister_overlay,
};

use crate::context::{attach_overlay, track_layout};
use crate::input_region::{InputHandle, receives_input, tracks_hidden, visible_rect, withhold};
use crate::layout_item::{LayoutItem, TrackedChildren, register_container};
use crate::pointer::{dispatch_container_event, offset_pointer};
use crate::surface_context::HomeSurface;

/// Where an anchored overlay's content sits relative to its trigger widget. Maps to the `.rsx` `placement` attribute. Only vertical placements are provided today; horizontal ones would follow the same pattern.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    /// Content's top-left at the trigger's bottom-left — a menu dropping down from its button.
    Below,
    /// Content's bottom-left at the trigger's top-left — a menu opening upward.
    Above,
    /// Beside the trigger on its leading side, centred on it — where a tooltip goes when the trigger sits in a vertical rail and the room is sideways.
    Start,
    /// Beside the trigger on its trailing side, centred on it.
    End,
}

/// The world-vs-local anchor fallback shared by the anchored menu/select/tooltip panels.
///
/// Uses the trigger's *on-screen* rect, not its laid-out one: a trigger inside a scrolled viewport is drawn somewhere other than where it was laid out, and a panel placed at the laid-out spot lands off by the scroll offset.
pub fn anchor_rect(node: NodeId, fallback: &RwSignal<Rect>) -> Rect {
    visible_rect(node).unwrap_or_else(|| fallback.peek())
}

/// Anchors an overlay's content to a trigger widget, placed off the trigger's on-screen rect. `laid` is its laid-out rect (what `track_layout` returns); reading it in `view()` makes the content follow the trigger across relayouts.
#[derive(Clone)]
struct Anchor {
    trigger: NodeId,
    laid: RwSignal<Rect>,
    placement: Placement,
}

impl Anchor {
    fn on_screen(&self, read: impl Fn(&RwSignal<Rect>) -> Rect) -> Rect {
        let laid = read(&self.laid);
        visible_rect(self.trigger).unwrap_or(laid)
    }
}

/// The panel box: the union of the children's laid-out rects (their intrinsic size before anchoring). `read` is `peek` during event routing (untracked) and `get` inside `view()` (so the render follows layout).
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

/// Gap kept between an anchored panel and the edge it was pushed off, so a shifted bubble does not sit flush against the window.
const EDGE_MARGIN: f32 = 4.0;

/// Gap kept between an anchored panel and the thing it is anchored to.
///
/// A panel flush against its trigger reads as *part of* the trigger, which is exactly what it is not: it is a separate surface that appeared because of it. The gap is what makes a menu look attached to its button rather than grown out of it — 4px is enough to separate the two surfaces and small enough that they still read as one gesture, which is why it is the offset most anchored-panel libraries settle on.
const ANCHOR_GAP: f32 = 4.0;

/// The area an anchored panel has to stay inside. Falls back to an unbounded box before the host has been laid out (the very first frame), where clamping to nothing is the same as not clamping.
fn anchor_viewport() -> Rect {
    crate::context::overlay_viewport().unwrap_or(Rect::new(0.0, 0.0, f32::MAX, f32::MAX))
}

/// The translate that moves `panel` from where it was laid out (near the host origin) to its anchored spot next to `trigger`, by the rule [`Anchoring::place`] states.
fn anchor_translate(
    trigger: Rect,
    panel: Rect,
    placement: Placement,
    viewport: Rect,
) -> (f32, f32) {
    let spot = Anchoring::new(trigger, placement, viewport)
        .gap(ANCHOR_GAP)
        .place(Size::new(panel.width, panel.height));
    ((spot.x - panel.x).round(), (spot.y - panel.y).round())
}

/// Which part of the anchor's edge a panel lines up with, across the side it opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorAlign {
    /// The anchor's leading edge: a menu under its button, a submenu level with its row.
    Start,
    /// The anchor's middle: a tooltip beside a button in a rail.
    Center,
}

/// How a panel is set beside the thing it belongs to and kept inside the area it may occupy: one rule for every anchored panel, whether a transform moves it there (a tooltip) or it is sized to the room it has (a dropdown's list, a context menu and its submenus).
///
/// A panel is placed from its anchor and then kept on screen. It **flips** to the other side when the asked-for one is too small for it and the other has more room, it is **capped** to the room on the side it ends up on — which is what makes a long list scroll instead of running off the surface — and it **shifts** along the anchor's edge when it overflows across it. Without the second half a tooltip on the rightmost button of a toolbar runs past the window, which is not a rare case but the common one. The anchor is never covered: a flip moves the panel to its other side, and a shift only slides along the edge it is already on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchoring {
    /// What the panel opens beside, as drawn: a trigger's on-screen rect, or a rect of no size at the point a context menu was asked for.
    pub anchor: Rect,
    pub placement: Placement,
    pub align: AnchorAlign,
    /// Between the anchor and the panel.
    pub gap: f32,
    /// The area the panel has to stay inside: the overlay's, see [`overlay_viewport`](crate::overlay_viewport).
    pub viewport: Rect,
}

impl Anchoring {
    /// No gap, and the alignment `placement` reads as: a panel above or below its anchor starts where the anchor starts, one beside it is centred on it.
    pub fn new(anchor: Rect, placement: Placement, viewport: Rect) -> Self {
        let align = match placement {
            Placement::Below | Placement::Above => AnchorAlign::Start,
            Placement::Start | Placement::End => AnchorAlign::Center,
        };
        Self {
            anchor,
            placement,
            align,
            gap: 0.0,
            viewport,
        }
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn align(mut self, align: AnchorAlign) -> Self {
        self.align = align;
        self
    }

    /// The largest panel [`place`](Self::place) sets down without leaving the viewport: the roomier side of the anchor along the placement, the whole viewport across it.
    ///
    /// What a panel is capped to *before* it is laid out, so it can be sized by its content and placed after. The roomier side and not the side it opens on, because that side is not known until its size is — and it does not have to be: a panel that fits where it was asked to open is no bigger than the room there, and one that does not opens on the roomier side, which is this.
    pub fn room(&self) -> Size {
        let (after, before) = self.rooms();
        let along = after.max(before);
        match self.placement {
            Placement::Below | Placement::Above => Size::new(across(self.viewport.width), along),
            Placement::Start | Placement::End => Size::new(along, across(self.viewport.height)),
        }
    }

    /// Where a panel of `size` goes and how big it may be there: the rect to draw it in, on whole pixels.
    pub fn place(&self, size: Size) -> Rect {
        let (after, before) = self.rooms();
        let vertical = matches!(self.placement, Placement::Below | Placement::Above);
        let (along, across_size) = match vertical {
            true => (size.height, size.width),
            false => (size.width, size.height),
        };
        let asked_after = matches!(self.placement, Placement::Below | Placement::End);
        let (asked, other) = match asked_after {
            true => (after, before),
            false => (before, after),
        };
        let stays = along <= asked || asked >= other;
        let opens_after = asked_after == stays;
        let along = along.min(if opens_after { after } else { before });

        let (a, v) = (self.anchor, self.viewport);
        let (a_start, a_extent, a_cross, a_cross_extent, v_cross, v_cross_extent) = match vertical {
            true => (a.y, a.height, a.x, a.width, v.x, v.width),
            false => (a.x, a.width, a.y, a.height, v.y, v.height),
        };
        let main = match opens_after {
            true => a_start + a_extent + self.gap,
            false => a_start - self.gap - along,
        };
        let across_size = across_size.min(across(v_cross_extent));
        let cross = match self.align {
            AnchorAlign::Start => a_cross,
            AnchorAlign::Center => a_cross + (a_cross_extent - across_size) / 2.0,
        };
        // Slid along the edge rather than clamped blindly, so a panel wider than the viewport keeps its leading edge — where its content starts — visible. LTR throughout, so RTL is one place to change.
        let cross = cross
            .min(v_cross + v_cross_extent - EDGE_MARGIN - across_size)
            .max(v_cross + EDGE_MARGIN);
        // A centred panel lands on a half pixel whenever the two differ by an odd size, and a surface at a half pixel has soft edges and softer text.
        let (main, cross) = (main.round(), cross.round());
        match vertical {
            true => Rect::new(cross, main, across_size, along),
            false => Rect::new(main, cross, along, across_size),
        }
    }

    /// The room past the anchor's far edge along the placement (below it, or after it) and before its near edge, each short of the gap and the edge margin.
    fn rooms(&self) -> (f32, f32) {
        let (a, v) = (self.anchor, self.viewport);
        let (a_start, a_end, v_start, v_end) = match self.placement {
            Placement::Below | Placement::Above => (a.y, a.y + a.height, v.y, v.y + v.height),
            Placement::Start | Placement::End => (a.x, a.x + a.width, v.x, v.x + v.width),
        };
        let after = v_end - EDGE_MARGIN - (a_end + self.gap);
        let before = a_start - self.gap - (v_start + EDGE_MARGIN);
        (after.max(0.0), before.max(0.0))
    }
}

/// What a viewport `extent` across leaves a panel once the edge margin is kept on both sides.
fn across(extent: f32) -> f32 {
    (extent - EDGE_MARGIN * 2.0).max(0.0)
}

/// The content rect an anchored overlay actually occupies on screen: its panel translated to the trigger. This is the hit-test barrier the registry sees, so only the visible panel blocks — clicks elsewhere fall through even though the underlying content node fills the viewport. Where the anchored panel ends up, and the translate that put it there.
///
/// Both answers come from one derivation — the panel union, then the flip/shift against the viewport — and both are wanted for the same pointer event: the registry hit-tests against the rect, and the dispatcher maps the event back into the children's space by the translate. Derived separately, the two could disagree about where the panel is while agreeing that the pointer was over it.
fn anchored_placement(
    children: &TrackedChildren,
    anchor: &Anchor,
    read: impl Fn(&RwSignal<Rect>) -> Rect,
) -> (Rect, (f32, f32)) {
    let panel = panel_rect(children, &read);
    let (dx, dy) = anchor_translate(
        anchor.on_screen(&read),
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

/// The overlay's hook into priority pointer routing. Shares the same `Rc<RefCell>` child handles as the `Overlay` widget (`Child` is a cheap clonable handle), so a pointer event dispatched through the sink reaches the very same content the widget renders. `content_rect` is the content container's layout rect, used as the hit-test barrier (a full-viewport scrim blocks everything; an anchored panel only itself).
struct OverlaySinkImpl {
    content_rect: RwSignal<Rect>,
    children: RefCell<TrackedChildren>,
    // Modal (swallow every event over the barrier) vs click-through (only where a child handled it).
    blocking: bool,
    // When set, the barrier and dispatch coordinates track the trigger instead of the fill container.
    anchor: Option<Anchor>,
    content: NodeId,
}

impl OverlaySink for OverlaySinkImpl {
    fn content_rect(&self) -> Rect {
        if !receives_input(self.content) {
            return Rect::default();
        }
        // `peek`, not `get`: routing runs during batched event dispatch, not inside a tracking effect.
        match &self.anchor {
            None => self.content_rect.peek(),
            Some(anchor) => anchored_content_rect(&self.children.borrow(), anchor, |s| s.peek()),
        }
    }

    fn dispatch(&self, event: &Event) -> EventResult {
        if !receives_input(self.content) {
            return withhold(event, |event| {
                dispatch_container_event(&mut self.children.borrow_mut(), event)
            });
        }
        // Anchored content is laid out at its intrinsic origin but hit at the anchored spot, so map the world event back into the children's local space first.
        let offset = self
            .anchor
            .as_ref()
            .map(|anchor| anchored_placement(&self.children.borrow(), anchor, |s| s.peek()).1);
        match offset {
            Some((dx, dy)) => {
                // `offset_pointer` applies the inverse of the translate, so the sign is positive. Negating it double-adds the anchor offset and mishits rows.
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

/// A portal layer: its content is laid out out-of-flow, filling the viewport, and hoisted to the top at compose time — drawn above everything and free of any ancestor clip/transform. A base primitive: unstyled; wrap content in a `box` for a scrim/panel, and position it with normal flex (`align`/`justify`).
///
/// The content is a separate layout node **attached to the layout root** (the overlay host), not to the widget's DOM parent — so a portal declared deep in the tree (e.g. inside a reactive `if`) still covers the whole window instead of collapsing to its parent's box. The widget hands its DOM parent only an empty placeholder, which takes no room in the flow, so it never affects sibling layout. A portal open before its surface has been laid out — built open, with its tree — waits for the host and is attached on the first layout pass.
///
/// Positioned pointer events reach the content with priority via a thread-local overlay registry (see `ui_tree::overlay_dispatch`): a click on the overlay is routed here before the main tree walk and does not fall through to the content behind it, so a scrim that fills the viewport reads as a modal.
///
/// Variants (all portal the same way, they differ in how they route clicks and where the content sits):
/// - [`Overlay::new`] — modal: blocks every click inside its content rect (a full-viewport scrim).
/// - [`Overlay::anchored_click_through`] — positions the content next to a trigger widget (dropdowns, menus, tooltips) and takes no pointer, so clicks anywhere fall through to the tree behind.
pub struct Overlay {
    // Handed to the DOM parent: an empty placeholder when portaled, the content itself otherwise.
    layout_node: NodeId,
    // The viewport-filling content node; `Some` and attached to the host only when portaled.
    portaled_content: Option<NodeId>,
    children: TrackedChildren,
    // Registry id for priority pointer routing; removed on drop.
    overlay_id: u64,
    // Focus-scope registration, withdrawn on drop alongside the pointer one.
    focus_scope: crate::focus::ScopeId,
    // Set for `anchored`: translates the rendered content to the trigger's rect.
    anchor: Option<Anchor>,
    // Kept mounted while false, so a modal's slotted body survives a close and reopen instead of being rebuilt from a consumed slot.
    visible: Rc<dyn Fn() -> bool>,
    home: HomeSurface,
    _input: InputHandle,
}

impl Overlay {
    /// A modal portal: the content fills the viewport and blocks every click behind it.
    pub fn new(
        layout_style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        Self::build(layout_style, children, true, None, Rc::new(|| true))
    }

    /// A modal portal that is kept mounted and shown/hidden by `visible` (read each frame). Unlike disposing and rebuilding the overlay on every open, this preserves its content across close/reopen — needed for a dialog whose body arrives as a pre-built slot (which cannot be rebuilt once consumed). Hidden, it draws nothing and blocks nothing.
    pub fn toggleable(
        layout_style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
        visible: impl Fn() -> bool + 'static,
    ) -> Result<Self, LayoutError> {
        Self::build(layout_style, children, true, None, Rc::new(visible))
    }

    /// A portal whose content is positioned next to `trigger` (a dropdown/menu/tooltip popping up by its button) and takes no pointer: a tooltip bubble, a hint, anything that appears because the pointer is *near* it and would be dismissed by touching it. The content sizes to its intrinsic panel and is translated to the trigger's rect per `placement` — the rect it is drawn at, so a trigger moved by a transform or a scroll still gets its panel beside it.
    pub fn anchored_click_through(
        layout_style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
        trigger: NodeId,
        placement: Placement,
    ) -> Result<Self, LayoutError> {
        let laid = track_layout(trigger).ok_or_else(|| {
            LayoutError::Engine("an anchored overlay's trigger has no layout node".to_string())
        })?;
        Self::build(
            layout_style,
            children,
            false,
            Some(Anchor {
                trigger,
                laid,
                placement,
            }),
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
        // `absolute_fill` takes the layer out of flow and sizes it to its container, which attaching to the host makes the viewport. An anchored overlay instead sizes intrinsically and moves with a transform.
        let (content, content_rect, children) =
            register_container(layout_style.absolute_fill(), children)?;

        // The sink shares the same child handles as the widget.
        let sink: Rc<dyn OverlaySink> = Rc::new(OverlaySinkImpl {
            content_rect,
            children: RefCell::new(children.clone()),
            blocking,
            anchor: anchor.clone(),
            content,
        });
        let overlay_id = register_overlay(sink);
        // Named by node rather than by the ids inside it: the children were built before this overlay existed, so ancestry has to answer at the moment Tab is pressed.
        let focus_scope = crate::focus::register_scope(
            content,
            {
                let visible = visible.clone();
                move || visible()
            },
            blocking,
        );
        let mut input = InputHandle::new();
        input.gate(content, None, Some(visible.clone()));
        if blocking {
            input.answer(content, content_rect.read_only());
        }
        if let Some(anchor) = &anchor {
            let (children, anchor) = (children.clone(), anchor.clone());
            input.place(
                content,
                crate::input_region::Placement::Offset(Rc::new(move || {
                    anchored_placement(&children, &anchor, |s| s.peek()).1
                })),
            );
        }

        if attach_overlay(content) {
            // An empty leaf takes no room in the flow, and left auto-sized it fills the space it is laid out in when the overlay is the root itself, which then hosts the portal.
            let (placeholder, _r) = crate::context::new_leaf(LayoutStyle::new())?;
            layout_reactive::free_with(content, placeholder);
            input.link(content, placeholder, true);
            Ok(Overlay {
                layout_node: placeholder,
                portaled_content: Some(content),
                children,
                overlay_id,
                focus_scope,
                anchor,
                visible,
                home: HomeSurface::here(),
                _input: input,
            })
        } else {
            // Refused by a host that sits inside the content, so the content covers its parent rather than the viewport.
            Ok(Overlay {
                layout_node: content,
                portaled_content: None,
                children,
                overlay_id,
                focus_scope,
                anchor,
                visible,
                home: HomeSurface::here(),
                _input: input,
            })
        }
    }
}

impl Overlay {
    /// Declares the control this overlay was opened from, which stays in what a reader is told while the overlay holds focus inside itself. See [`focus::set_scope_owner`](crate::focus::set_scope_owner).
    pub fn owned_by(self, owner: NodeId) -> Self {
        crate::focus::set_scope_owner(self.focus_scope, owner);
        self
    }

    /// The node its content actually hangs from, which is the portaled one unless the host refused it. What a caller asks for to reason about the content by ancestry — autofocusing what is inside it, say — since [`layout_node`](LayoutItem::layout_node) is an empty placeholder once portaled.
    pub fn content_node(&self) -> NodeId {
        self.portaled_content.unwrap_or(self.layout_node)
    }
}

impl LayoutItem for Overlay {
    fn layout_node(&self) -> NodeId {
        self.layout_node
    }

    /// Reached through the overlay registry ahead of the tree walk, so its in-tree node never covers its siblings, which matters when the host refused the content and it is laid out in place.
    fn occludes(&self) -> bool {
        false
    }
}

impl Component for Overlay {
    fn view(&self) -> RenderNode {
        // Its content stays alive for the next time it is shown. A root of its own, it is not hidden by a box hiding where it was declared unless it asks.
        if !(self.visible)() || tracks_hidden(self.layout_node) {
            return RenderNode::Empty;
        }
        let boundaries = self.children.iter().map(|c| c.segment.boundary());
        match &self.anchor {
            None => RenderNode::overlay(boundaries),
            Some(anchor) => {
                // `get`, not `peek`, so the transform re-runs when the trigger or the panel's size changes.
                let panel = panel_rect(&self.children, |s| s.get());
                let (dx, dy) = anchor_translate(
                    anchor.on_screen(|s| s.get()),
                    panel,
                    anchor.placement,
                    anchor_viewport(),
                );
                // The content is laid out at the origin and drawn at the trigger.
                RenderNode::overlay([RenderNode::transform_with(
                    [1.0, 0.0, 0.0, 1.0, dx, dy],
                    boundaries,
                )])
            }
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // Positioned pointer events already arrive with priority through the overlay registry, so dispatching them here too would double-fire. Non-positioned ones still flow through the tree.
        if matches!(
            event,
            Event::PointerPressed { .. }
                | Event::PointerMoved { .. }
                | Event::PointerReleased { .. }
        ) {
            return EventResult::Ignored;
        }
        if !receives_input(self.content_node()) {
            return withhold(event, |event| {
                dispatch_container_event(&mut self.children, event)
            });
        }
        dispatch_container_event(&mut self.children, event)
    }

    fn debug_name(&self) -> &'static str {
        "Overlay"
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        let Some(_home) = self.home.enter() else {
            return;
        };
        unregister_overlay(self.overlay_id);
        crate::focus::unregister_scope(self.focus_scope);
    }
}

#[cfg(test)]
impl Overlay {
    // The on-screen content rect an anchored overlay presents to the registry as its hit-test barrier.
    fn anchored_barrier(&self) -> Rect {
        let anchor = self.anchor.as_ref().expect("overlay is not anchored");
        anchored_content_rect(&self.children, anchor, |s| s.peek())
    }
}

#[cfg(test)]
#[path = "overlay_test.rs"]
mod tests;
