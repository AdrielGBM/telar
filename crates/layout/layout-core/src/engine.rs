//! The layout engine: a taffy tree behind an id-keyed API, plus the measure hooks and dirty tracking the reactive layer drives it with.

use rustc_hash::{FxHashMap, FxHashSet};
use taffy::{TaffyTree, TraversePartialTree};

use geometry_core::Size;

use crate::direction::Direction;
use crate::error::LayoutError;
use crate::sticky::StickyInsets;
use crate::style::{AvailableSpace, LayoutStyle};

/// A node in the layout tree. Ids are reused after a node is freed, so a stale one may name a live node.
pub type NodeId = taffy::NodeId;

/// Per-node measure callback: given the available main-axis width, returns the node's intrinsic (width, height). Used for text nodes whose height depends on how many lines the content wraps into at the resolved width.
pub type MeasureFn = Box<dyn FnMut(f32) -> (f32, f32)>;

/// The layout tree: nodes, their styles, and the measure hooks for the leaves that size themselves.
pub struct LayoutEngine {
    tree: TaffyTree<MeasureFn>,
    direction: Direction,
    surface: Size,
    /// What every node was asked for, which every path pushing a style to taffy resolves from. See [`current_style`](Self::current_style) for why it is every node and not only those needing it.
    styles: FxHashMap<NodeId, LayoutStyle>,
    /// Every node this engine currently owns. Kept because taffy has no total way to ask: `style()` indexes its slot map and panics on a freed key rather than answering, so there is nothing to guard with. See [`alive`](Self::alive) for why anything asks at all.
    live: FxHashSet<NodeId>,
    /// The insets of every sticky node as last resolved, which taffy never sees.
    sticky: FxHashMap<NodeId, StickyInsets>,
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            tree: TaffyTree::new(),
            direction: Direction::default(),
            surface: Size::ZERO,
            styles: FxHashMap::default(),
            live: FxHashSet::default(),
            sticky: FxHashMap::default(),
        }
    }

    /// The direction logical edges currently resolve against.
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Re-resolves every direction-dependent node against `direction`, returning whether anything changed. The caller still has to mark the tree dirty and recompute — this only rewrites styles.
    ///
    /// This is what lets one build serve both directions: rather than rebuilding the widget tree, each node that was authored logically is resolved again from the intent recorded when it was created.
    pub fn set_direction(&mut self, direction: Direction) -> bool {
        if self.direction == direction {
            return false;
        }
        self.direction = direction;
        let styles = std::mem::take(&mut self.styles);
        for (&node, style) in &styles {
            self.push_style(node, style);
        }
        self.styles = styles;
        true
    }

    /// The size fractions of the surface currently resolve against.
    pub fn surface_size(&self) -> Size {
        self.surface
    }

    /// Re-resolves every node written as a fraction of the surface against `surface`, returning whether any was. Like [`set_direction`](Self::set_direction) this only rewrites styles, and only those that name the surface: every other node keeps its cached layout.
    pub fn set_surface_size(&mut self, surface: Size) -> bool {
        if self.surface == surface {
            return false;
        }
        self.surface = surface;
        let styles = std::mem::take(&mut self.styles);
        let mut resolved = false;
        for (&node, style) in styles
            .iter()
            .filter(|(_, style)| style.is_surface_relative())
        {
            self.push_style(node, style);
            resolved = true;
        }
        self.styles = styles;
        resolved
    }

    /// Records the intent this node was built from, which every later push resolves again.
    fn track(&mut self, node: NodeId, style: LayoutStyle) {
        self.live.insert(node);
        self.styles.insert(node, style);
    }

    /// The intent this node was built from.
    ///
    /// Every node keeps one — including those needing nothing resolved — so this is exact rather than reconstructed. It costs one style per node, which is what taffy already holds, and buys two things: a direction flip re-resolves from what was written instead of trying to un-swap edges it can no longer tell apart, and a backend whose output is a document can ask what a box was *asked* for rather than only where it ended up.
    fn current_style(&self, node: NodeId) -> LayoutStyle {
        self.styles.get(&node).cloned().unwrap_or_default()
    }

    /// The intent `node` was built from, for a caller that wants to read it rather than change it.
    pub fn declared_style(&self, node: NodeId) -> Option<&LayoutStyle> {
        self.styles.get(&node)
    }

    /// Resolves `style` and pushes it to taffy, placing the leading margin — `resolve` cannot, since that needs the parent's axis to know which physical edge is "leading".
    fn push_style(&mut self, node: NodeId, style: &LayoutStyle) {
        let mut resolved = style.resolve(self.direction, self.surface);
        self.note_sticky(node, style);
        if let Some((is_row, px)) = style.logical.leading_margin {
            let leading_right = is_row && self.leads_from_right(node);
            let m = taffy::LengthPercentageAuto::length(px);
            if is_row {
                if leading_right {
                    resolved.margin.right = m;
                } else {
                    resolved.margin.left = m;
                }
            } else {
                resolved.margin.top = m;
            }
        }
        let _ = self.tree.set_style(node, resolved);
    }

    /// Shared plumbing for every out-of-band mutator: mutate one field of the tracked style, push, re-track.
    fn mutate_style(&mut self, node: NodeId, f: impl FnOnce(&mut LayoutStyle)) {
        if !self.live.contains(&node) {
            return;
        }
        let mut style = self.current_style(node);
        f(&mut style);
        self.push_style(node, &style);
        self.track(node, style);
    }

    fn forget(&mut self, node: NodeId) {
        self.live.remove(&node);
        self.styles.remove(&node);
        self.sticky.remove(&node);
    }

    fn note_sticky(&mut self, node: NodeId, style: &LayoutStyle) {
        match style.sticky_insets(self.direction, self.surface) {
            Some(insets) => self.sticky.insert(node, insets),
            None => self.sticky.remove(&node),
        };
    }

    pub fn new_leaf(&mut self, style: LayoutStyle) -> Result<NodeId, LayoutError> {
        let node = self
            .tree
            .new_leaf(style.resolve(self.direction, self.surface))?;
        self.note_sticky(node, &style);
        self.track(node, style);
        Ok(node)
    }

    pub fn new_measured_leaf(
        &mut self,
        style: LayoutStyle,
        measure: MeasureFn,
    ) -> Result<NodeId, LayoutError> {
        let node = self
            .tree
            .new_leaf_with_context(style.resolve(self.direction, self.surface), measure)?;
        self.note_sticky(node, &style);
        self.track(node, style);
        Ok(node)
    }

    pub fn new_container(
        &mut self,
        style: LayoutStyle,
        children: &[NodeId],
    ) -> Result<NodeId, LayoutError> {
        let node = self
            .tree
            .new_with_children(style.resolve(self.direction, self.surface), children)?;
        self.note_sticky(node, &style);
        self.track(node, style);
        Ok(node)
    }

    /// Replaces `node`'s declared style, carrying forward whatever the out-of-band mutators below set — a freshly-built `style` (e.g. from a `styled_by` closure reacting to an unrelated signal) has no way to know about them.
    pub fn set_style(&mut self, node: NodeId, mut style: LayoutStyle) -> Result<(), LayoutError> {
        self.alive(node)?;
        if let Some(previous) = self.styles.get(&node) {
            // The out-of-band answer only: a fresh style cannot know it was ever given, so it is carried forward. What the style says about being shown is left alone, since OR-ing the two made a node hidden once hidden forever.
            style.logical.display_override = style
                .logical
                .display_override
                .or(previous.logical.display_override);
            style.logical.row_forced |= previous.logical.row_forced;
            style.logical.min_height_override = style
                .logical
                .min_height_override
                .or(previous.logical.min_height_override);
            style.logical.leading_margin = style
                .logical
                .leading_margin
                .or(previous.logical.leading_margin);
        }
        self.push_style(node, &style);
        self.track(node, style);
        Ok(())
    }

    /// Replaces `parent`'s children with `children`, in order. Used by reactive lists to insert, move, and drop item nodes as their source collection changes.
    pub fn set_children(&mut self, parent: NodeId, children: &[NodeId]) -> Result<(), LayoutError> {
        self.alive(parent)?;
        for &child in children {
            self.alive(child)?;
        }
        self.tree
            .set_children(parent, children)
            .map_err(LayoutError::from)
    }

    /// Appends `child` to `parent`'s existing children (unlike [`set_children`](Self::set_children), which replaces them). Used to attach an overlay's out-of-flow content to the layout root without touching the root's other children.
    pub fn add_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), LayoutError> {
        self.alive(parent)?;
        self.alive(child)?;
        self.tree
            .add_child(parent, child)
            .map_err(LayoutError::from)
    }

    /// Detaches `child` from `parent` (does not free it — call [`remove`](Self::remove) afterwards to release the node).
    pub fn remove_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), LayoutError> {
        self.alive(parent)?;
        self.alive(child)?;
        self.tree
            .remove_child(parent, child)
            .map(|_| ())
            .map_err(LayoutError::from)
    }

    /// The node's style, or `None` once it has been freed — the read-side twin of [`alive`](Self::alive).
    fn style_of(&self, node: NodeId) -> Option<&taffy::Style> {
        if !self.live.contains(&node) {
            return None;
        }
        self.tree.style(node).ok()
    }

    /// An error, rather than a panic, for a node that has been freed.
    ///
    /// Every mutator goes through this because a widget can legitimately outlive its node: a `Segment` holds the widget so a re-render mid-dispatch can still flatten it, so an effect the widget owns may fire once after the list that held it dropped the node. Taffy indexes its slot map directly and would panic on that id — and these all return `Result` already, so the honest answer to "attach a node that is gone" is the error the signature promises. The read-only queries take the same view (`is_size_auto` and friends already fall back rather than fail).
    fn alive(&self, node: NodeId) -> Result<(), LayoutError> {
        if self.live.contains(&node) {
            Ok(())
        } else {
            Err(LayoutError::Engine(format!(
                "node {node:?} no longer exists"
            )))
        }
    }

    /// Frees a node (and its measure context) from the tree. The caller must have already detached it from its parent (via [`set_children`](Self::set_children)); a removed node id must not be used again.
    ///
    /// **Freeing one twice is a no-op, not a crash.** Two owners can reach the same node without seeing each other — a `ReactiveList` reconciling away a row it no longer has, and the row's own owner being disposed — and both are right to free what they held. Taffy panics on a key it has already handed back (`invalid SlotMap key`), so the second free took the process down with it.
    pub fn remove(&mut self, node: NodeId) {
        if self.alive(node).is_err() {
            return;
        }
        self.forget(node);
        let _ = self.tree.remove(node);
    }

    /// The nodes `node` currently holds, in order; empty once it has been freed.
    pub fn children(&self, node: NodeId) -> Vec<NodeId> {
        if self.alive(node).is_err() {
            return Vec::new();
        }
        self.tree.children(node).unwrap_or_default()
    }

    pub fn node_count(&self) -> usize {
        self.live.len()
    }

    pub fn mark_dirty(&mut self, node: NodeId) -> Result<(), LayoutError> {
        self.alive(node)?;
        self.tree.mark_dirty(node).map_err(LayoutError::from)
    }

    /// Whether the node's `width`/`height` are `auto` (i.e. content-sized).
    pub fn is_size_auto(&self, node: NodeId) -> (bool, bool) {
        match self.style_of(node) {
            Some(s) => (s.size.width.is_auto(), s.size.height.is_auto()),
            None => (false, false),
        }
    }

    /// Sets the node's width to a definite length, or back to `auto` when `None`.
    pub fn set_width(&mut self, node: NodeId, width: Option<f32>) {
        self.mutate_style(node, |style| style.set_definite_width(width));
    }

    /// Sets the node's height to a definite length, or back to `auto` when `None`.
    pub fn set_height(&mut self, node: NodeId, height: Option<f32>) {
        self.mutate_style(node, |style| style.set_definite_height(height));
    }

    /// Sets the node's minimum height to a definite length, or clears it (`auto`) when `None`. Lets a content-measured leaf (e.g. a code editor's text area) fill a viewport it would otherwise underflow.
    pub fn set_min_height(&mut self, node: NodeId, height: Option<f32>) {
        self.mutate_style(node, |style| {
            style.logical.min_height_override = height;
        });
    }

    /// Turns `node` into a flex row after construction, registering it as direction-following so a later RTL flip reverses it like an authored `flex_row`. What a reconciling list calls when it learns — from the container it is being attached to — that its items run horizontally.
    pub fn make_flex_row(&mut self, node: NodeId) {
        self.mutate_style(node, |style| {
            style.logical.row_forced = true;
        });
    }

    /// Whether the node lays its children along the main (horizontal) axis — a flex row. A column, or any non-row node (missing / errored), is `false`. A transparent fragment reads its host's axis to know which margin edge a per-item gap sits on.
    pub fn is_row(&self, node: NodeId) -> bool {
        // Same guard every other read takes, and for the same reason: taffy panics on a freed key rather than reporting it, so a node removed since the caller last saw it has to be answered here.
        if self.alive(node).is_err() {
            return false;
        }
        self.tree
            .style(node)
            .map(|s| {
                matches!(
                    s.flex_direction,
                    taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse
                )
            })
            .unwrap_or(false)
    }

    /// Sets the node's leading margin on the host's main axis (`top` for a column; for a row, whichever horizontal edge the host lays out from) to `px`, leaving the other edges untouched. A transparent `for … gap:N` uses this to space its items by a gap without a container of its own: the item cell carries the gap as a margin instead.
    pub fn set_leading_margin(&mut self, node: NodeId, is_row: bool, px: f32) {
        self.mutate_style(node, |style| {
            style.logical.leading_margin = Some((is_row, px));
        });
    }

    /// Whether the node's host lays its children out from the right — an RTL row, or one explicitly reversed.
    fn leads_from_right(&self, node: NodeId) -> bool {
        if !self.live.contains(&node) {
            return false;
        }
        self.tree
            .parent(node)
            .and_then(|parent| self.style_of(parent))
            .map(|s| s.flex_direction == taffy::FlexDirection::RowReverse)
            .unwrap_or(false)
    }

    /// Whether this node is itself out of layout flow. Says nothing about its ancestors — [`walk`](Self::walk) carries that down as it descends, and a caller asking about one node has to climb for itself.
    pub fn is_display_none(&self, node: NodeId) -> bool {
        self.style_of(node)
            .map(|s| s.display == taffy::Display::None)
            .unwrap_or(false)
    }

    /// Toggles a node in or out of layout flow. A hidden node (`Display::None`) takes no space and lays out none of its subtree; a visible node returns to whichever `display` it declared. Used for responsive show/hide (e.g. collapsing a sidebar on narrow windows).
    pub fn set_display(&mut self, node: NodeId, visible: bool) {
        self.mutate_style(node, |style| {
            style.logical.display_override = Some(visible);
        });
    }

    pub fn compute_layout(
        &mut self,
        root: NodeId,
        available_width: AvailableSpace,
        available_height: AvailableSpace,
    ) -> Result<(), LayoutError> {
        self.alive(root)?;
        self.tree
            .compute_layout_with_measure(
                root,
                taffy::geometry::Size {
                    width: available_width,
                    height: available_height,
                },
                |inputs, _node, context, style| {
                    taffy::compute_leaf_layout(
                        inputs,
                        style,
                        |_, _| 0.0,
                        |known, available| {
                            let Some(measure) = context else {
                                return taffy::geometry::Size::ZERO;
                            };
                            // Width to wrap against: a resolved width wins, else the definite available width, else a large bound so MaxContent stays single-line.
                            let width = known.width.unwrap_or(match available.width {
                                taffy::AvailableSpace::Definite(w) => w,
                                taffy::AvailableSpace::MaxContent => 1.0e6,
                                taffy::AvailableSpace::MinContent => 0.0,
                            });
                            let (mw, mh) = measure(width);
                            taffy::geometry::Size {
                                width: known.width.unwrap_or(mw),
                                height: known.height.unwrap_or(mh),
                            }
                        },
                    )
                },
            )
            .map_err(LayoutError::from)
    }

    pub fn is_dirty(&self, node: NodeId) -> bool {
        self.live.contains(&node) && self.tree.dirty(node).unwrap_or(true)
    }

    pub fn layout(&self, node: NodeId) -> Result<geometry_core::Rect, LayoutError> {
        self.alive(node)?;
        let layout = self.tree.layout(node).map_err(LayoutError::from)?;
        Ok(geometry_core::Rect::new(
            layout.location.x,
            layout.location.y,
            layout.size.width,
            layout.size.height,
        ))
    }

    /// Every node under `root`, top-down, at the root-relative rect layout gave it; `f` returns whether to descend. Sticky nodes are left where layout put them — see [`walk_in_view`](Self::walk_in_view).
    pub fn walk<F>(&self, root: NodeId, f: &mut F) -> Result<(), LayoutError>
    where
        F: FnMut(NodeId, geometry_core::Rect) -> bool,
    {
        self.alive(root)?;
        self.visit(Visit::root(root), None, f, &mut Vec::new())
    }

    /// [`walk`](Self::walk), with every sticky node displaced for `view` — the root-relative rect the tree is seen through, such as a scroll viewport's visible window over its content — and its subtree displaced with it.
    ///
    /// Returns the outermost sticky nodes it met, so that when only the view moves [`walk_anchor`](Self::walk_anchor) can place their subtrees again without walking the rest of the tree, which does not move. A layout root is never sticky itself: it has no containing block to stick within.
    pub fn walk_in_view<F>(
        &self,
        root: NodeId,
        view: geometry_core::Rect,
        f: &mut F,
    ) -> Result<Vec<StickyAnchor>, LayoutError>
    where
        F: FnMut(NodeId, geometry_core::Rect) -> bool,
    {
        self.alive(root)?;
        let mut anchors = Vec::new();
        self.visit(Visit::root(root), Some(view), f, &mut anchors)?;
        Ok(anchors)
    }

    /// Places the subtree `anchor` names for a new `view`, as [`walk_in_view`](Self::walk_in_view) would have. Only valid while the layout it came from is: a relayout can move what stands above the anchor.
    pub fn walk_anchor<F>(
        &self,
        anchor: &StickyAnchor,
        view: geometry_core::Rect,
        f: &mut F,
    ) -> Result<(), LayoutError>
    where
        F: FnMut(NodeId, geometry_core::Rect) -> bool,
    {
        self.alive(anchor.node)?;
        let first = Visit {
            node: anchor.node,
            origin: anchor.origin,
            container: Some(anchor.container),
            hidden: false,
            inside_sticky: false,
        };
        self.visit(first, Some(view), f, &mut Vec::new())
    }

    fn visit<F>(
        &self,
        first: Visit,
        view: Option<geometry_core::Rect>,
        f: &mut F,
        anchors: &mut Vec<StickyAnchor>,
    ) -> Result<(), LayoutError>
    where
        F: FnMut(NodeId, geometry_core::Rect) -> bool,
    {
        let mut stack = Vec::with_capacity(64);
        stack.push(first);

        while let Some(visit) = stack.pop() {
            let layout = self.tree.layout(visit.node).map_err(LayoutError::from)?;
            let hidden = visit.hidden
                || self
                    .tree
                    .style(visit.node)
                    .map(|s| s.display == taffy::Display::None)
                    .unwrap_or(false);
            let (w, h) = if hidden {
                (0.0, 0.0)
            } else {
                (layout.size.width, layout.size.height)
            };
            let mut x = visit.origin.0 + layout.location.x;
            let mut y = visit.origin.1 + layout.location.y;

            let sticky = match (view, visit.container, self.sticky.get(&visit.node)) {
                (Some(view), Some(container), Some(insets)) if !hidden => {
                    Some((view, container, insets))
                }
                _ => None,
            };
            if let Some((view, container, insets)) = sticky {
                if !visit.inside_sticky {
                    anchors.push(StickyAnchor {
                        node: visit.node,
                        origin: visit.origin,
                        container,
                    });
                }
                let (dx, dy) = crate::sticky::displacement(
                    insets,
                    geometry_core::Rect::new(x, y, w, h),
                    layout.margin,
                    container,
                    view,
                );
                x += dx;
                y += dy;
            }

            if f(visit.node, geometry_core::Rect::new(x, y, w, h)) {
                let inset_x = layout.border.left + layout.padding.left;
                let inset_y = layout.border.top + layout.padding.top;
                let content = geometry_core::Rect::new(
                    x + inset_x,
                    y + inset_y,
                    (w - inset_x - layout.border.right - layout.padding.right).max(0.0),
                    (h - inset_y - layout.border.bottom - layout.padding.bottom).max(0.0),
                );
                let base = stack.len();
                for child in self.tree.child_ids(visit.node) {
                    stack.push(Visit {
                        node: child,
                        origin: (x, y),
                        container: Some(content),
                        hidden,
                        inside_sticky: visit.inside_sticky || sticky.is_some(),
                    });
                }
                stack[base..].reverse();
            }
        }
        Ok(())
    }
}

/// One node a walk has still to reach, with what it has worked out above it.
struct Visit {
    node: NodeId,
    /// Where the parent's border box ended up, which the node's layout location is relative to.
    origin: (f32, f32),
    /// The parent's content box: a sticky node's containing block. `None` at a layout root.
    container: Option<geometry_core::Rect>,
    /// Taffy stops laying out the subtree under a `display:none` ancestor, leaving stale layouts, so widgets drawing at fixed coordinates would still paint. The whole subtree is forced to a zero size instead.
    hidden: bool,
    /// Under a sticky node, whose displacement this node already carries.
    inside_sticky: bool,
}

impl Visit {
    fn root(node: NodeId) -> Self {
        Self {
            node,
            origin: (0.0, 0.0),
            container: None,
            hidden: false,
            inside_sticky: false,
        }
    }
}

/// A sticky node no other sticky node contains, with what a walk had worked out above it: enough to place its subtree again for another view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StickyAnchor {
    node: NodeId,
    origin: (f32, f32),
    container: geometry_core::Rect,
}

impl StickyAnchor {
    /// The sticky node itself.
    pub fn node(&self) -> NodeId {
        self.node
    }
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "engine_test.rs"]
mod tests;

#[cfg(test)]
#[path = "engine_sticky_test.rs"]
mod sticky_tests;
