use rustc_hash::{FxHashMap, FxHashSet};
use taffy::{TaffyTree, TraversePartialTree};

use crate::direction::Direction;
use crate::error::LayoutError;
use crate::style::{AvailableSpace, LayoutStyle};

pub type NodeId = taffy::NodeId;

/// Per-node measure callback: given the available main-axis width, returns the
/// node's intrinsic (width, height). Used for text nodes whose height depends on
/// how many lines the content wraps into at the resolved width.
pub type MeasureFn = Box<dyn FnMut(f32) -> (f32, f32)>;

pub struct LayoutEngine {
    tree: TaffyTree<MeasureFn>,
    direction: Direction,
    /// Every node's single current intent — a logical edge or out-of-band mutator state — that every path pushing a style to taffy resolves from; a node needing neither pays nothing (see [`LogicalStyle::needs_tracking`]).
    styles: FxHashMap<NodeId, LayoutStyle>,
    /// Plain direction-following rows, cheaper than a `styles` entry until one acquires other tracked state.
    directional_rows: FxHashSet<NodeId>,
    /// Every node this engine currently owns. Kept because taffy has no total way to ask: `style()` indexes
    /// its slot map and panics on a freed key rather than answering, so there is nothing to guard with. See
    /// [`alive`](Self::alive) for why anything asks at all.
    live: FxHashSet<NodeId>,
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            tree: TaffyTree::new(),
            direction: Direction::default(),
            styles: FxHashMap::default(),
            directional_rows: FxHashSet::default(),
            live: FxHashSet::default(),
        }
    }

    /// The direction logical edges currently resolve against.
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Re-resolves every direction-dependent node against `direction`, returning whether anything changed.
    /// The caller still has to mark the tree dirty and recompute — this only rewrites styles.
    ///
    /// This is what lets one build serve both directions: rather than rebuilding the widget tree, each node
    /// that was authored logically is resolved again from the intent recorded when it was created.
    pub fn set_direction(&mut self, direction: Direction) -> bool {
        if self.direction == direction {
            return false;
        }
        self.direction = direction;
        let rows = std::mem::take(&mut self.directional_rows);
        for &node in &rows {
            if let Some(current) = self.style_of(node) {
                let mut style = current.clone();
                style.flex_direction = if direction.is_rtl() {
                    taffy::FlexDirection::RowReverse
                } else {
                    taffy::FlexDirection::Row
                };
                let _ = self.tree.set_style(node, style);
            }
        }
        self.directional_rows = rows;
        let styles = std::mem::take(&mut self.styles);
        for (&node, style) in &styles {
            self.push_style(node, style);
        }
        self.styles = styles;
        true
    }

    /// Files `style` under `styles` or `directional_rows` depending on what it needs tracked for later.
    fn track(&mut self, node: NodeId, style: LayoutStyle) {
        self.live.insert(node);
        if style.logical.needs_tracking() {
            self.directional_rows.remove(&node);
            self.styles.insert(node, style);
            return;
        }
        self.styles.remove(&node);
        if style.logical.row_follows_direction {
            self.directional_rows.insert(node);
        } else {
            self.directional_rows.remove(&node);
        }
    }

    /// The node's tracked style, or one reconstructed from its live taffy style if it never needed tracking.
    fn current_style(&self, node: NodeId) -> LayoutStyle {
        if let Some(style) = self.styles.get(&node) {
            return style.clone();
        }
        LayoutStyle {
            inner: self.tree.style(node).cloned().unwrap_or_default(),
            logical: crate::style::LogicalStyle {
                row_follows_direction: self.directional_rows.contains(&node),
                ..Default::default()
            },
        }
    }

    /// Resolves `style` and pushes it to taffy, placing the leading margin — `resolve` cannot, since that needs the parent's axis to know which physical edge is "leading".
    fn push_style(&mut self, node: NodeId, style: &LayoutStyle) {
        let mut resolved = style.resolve(self.direction);
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
        self.directional_rows.remove(&node);
    }

    pub fn new_leaf(&mut self, style: LayoutStyle) -> Result<NodeId, LayoutError> {
        let node = self.tree.new_leaf(style.resolve(self.direction))?;
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
            .new_leaf_with_context(style.resolve(self.direction), measure)?;
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
            .new_with_children(style.resolve(self.direction), children)?;
        self.track(node, style);
        Ok(node)
    }

    /// Replaces `node`'s declared style, carrying forward whatever the out-of-band mutators below set — a freshly-built `style` (e.g. from a `styled_by` closure reacting to an unrelated signal) has no way to know about them.
    pub fn set_style(&mut self, node: NodeId, mut style: LayoutStyle) -> Result<(), LayoutError> {
        self.alive(node)?;
        if let Some(previous) = self.styles.get(&node) {
            style.logical.hidden |= previous.logical.hidden;
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

    /// Replaces `parent`'s children with `children`, in order. Used by reactive lists to insert, move,
    /// and drop item nodes as their source collection changes.
    pub fn set_children(&mut self, parent: NodeId, children: &[NodeId]) -> Result<(), LayoutError> {
        self.alive(parent)?;
        for &child in children {
            self.alive(child)?;
        }
        self.tree
            .set_children(parent, children)
            .map_err(LayoutError::from)
    }

    /// Appends `child` to `parent`'s existing children (unlike [`set_children`], which replaces them). Used
    /// to attach an overlay's out-of-flow content to the layout root without touching the root's other children.
    pub fn add_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), LayoutError> {
        self.alive(parent)?;
        self.alive(child)?;
        self.tree
            .add_child(parent, child)
            .map_err(LayoutError::from)
    }

    /// Detaches `child` from `parent` (does not free it — call [`remove`] afterwards to release the node).
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
    /// Every mutator goes through this because a widget can legitimately outlive its node: a `Segment` holds
    /// the widget so a re-render mid-dispatch can still flatten it, so an effect the widget owns may fire
    /// once after the list that held it dropped the node. Taffy indexes its slot map directly and would
    /// panic on that id — and these all return `Result` already, so the honest answer to "attach a node that
    /// is gone" is the error the signature promises. The read-only queries take the same view (`is_size_auto`
    /// and friends already fall back rather than fail).
    fn alive(&self, node: NodeId) -> Result<(), LayoutError> {
        if self.live.contains(&node) {
            Ok(())
        } else {
            Err(LayoutError::Engine(format!(
                "node {node:?} no longer exists"
            )))
        }
    }

    /// Frees a node (and its measure context) from the tree. The caller must have already detached it from
    /// its parent (via [`set_children`]); a removed node id must not be used again.
    ///
    /// **Freeing one twice is a no-op, not a crash.** Two owners can reach the same node without seeing each
    /// other — a `ReactiveList` reconciling away a row it no longer has, and the row's own owner being
    /// disposed — and both are right to free what they held. Taffy panics on a key it has already handed
    /// back (`invalid SlotMap key`), so the second free took the process down with it.
    pub fn remove(&mut self, node: NodeId) {
        if self.alive(node).is_err() {
            return;
        }
        self.forget(node);
        let _ = self.tree.remove(node);
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
        self.mutate_style(node, |style| {
            style.inner.size.width =
                width.map_or(taffy::Dimension::auto(), taffy::Dimension::length);
        });
    }

    /// Sets the node's height to a definite length, or back to `auto` when `None`.
    pub fn set_height(&mut self, node: NodeId, height: Option<f32>) {
        self.mutate_style(node, |style| {
            style.inner.size.height =
                height.map_or(taffy::Dimension::auto(), taffy::Dimension::length);
        });
    }

    /// Sets the node's minimum height to a definite length, or clears it (`auto`) when `None`. Lets a
    /// content-measured leaf (e.g. a code editor's text area) fill a viewport it would otherwise underflow.
    pub fn set_min_height(&mut self, node: NodeId, height: Option<f32>) {
        self.mutate_style(node, |style| {
            style.logical.min_height_override = height;
        });
    }

    /// Turns `node` into a flex row after construction, registering it as direction-following so a later
    /// RTL flip reverses it like an authored `flex_row`. What a reconciling list calls when it learns —
    /// from the container it is being attached to — that its items run horizontally.
    pub fn make_flex_row(&mut self, node: NodeId) {
        self.mutate_style(node, |style| {
            style.logical.row_forced = true;
        });
    }

    /// Whether the node lays its children along the main (horizontal) axis — a flex row. A column, or any
    /// non-row node (missing / errored), is `false`. A transparent fragment reads its host's axis to know
    /// which margin edge a per-item gap sits on.
    pub fn is_row(&self, node: NodeId) -> bool {
        // Same guard every other read takes, and for the same reason: taffy panics on a freed key rather
        // than reporting it, so a node removed since the caller last saw it has to be answered here.
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

    /// Sets the node's leading margin on the host's main axis (`top` for a column; for a row, whichever
    /// horizontal edge the host lays out from) to `px`, leaving the other edges untouched. A transparent
    /// `for … gap:N` uses this to space its items by a gap without a container of its own: the item cell
    /// carries the gap as a margin instead.
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

    /// Whether this node is itself out of layout flow. Says nothing about its ancestors — [`walk`](Self::walk)
    /// carries that down as it descends, and a caller asking about one node has to climb for itself.
    pub fn is_display_none(&self, node: NodeId) -> bool {
        self.style_of(node)
            .map(|s| s.display == taffy::Display::None)
            .unwrap_or(false)
    }

    /// Toggles a node in or out of layout flow. A hidden node (`Display::None`) takes no space and lays out none of its subtree; a visible node returns to whichever `display` it declared. Used for responsive show/hide (e.g. collapsing a sidebar on narrow windows).
    pub fn set_display(&mut self, node: NodeId, visible: bool) {
        self.mutate_style(node, |style| {
            style.logical.hidden = !visible;
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
                    width: available_width.into(),
                    height: available_height.into(),
                },
                |known, available, _node, context, _style| {
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

    pub fn is_fixed_size(&self, node: NodeId) -> Option<(f32, f32)> {
        let style = self.style_of(node)?;
        let w = style.size.width.into_option()?;
        let h = style.size.height.into_option()?;
        if style.flex_grow > 0.0 {
            return None;
        }
        Some((w, h))
    }

    pub fn collect_dirty_nodes(&self, root: NodeId, out: &mut Vec<NodeId>) {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if self.is_dirty(node) {
                out.push(node);
            }
            for child in self.tree.child_ids(node) {
                stack.push(child);
            }
        }
    }

    pub fn walk<F>(&self, root: NodeId, f: &mut F) -> Result<(), LayoutError>
    where
        F: FnMut(NodeId, geometry_core::Rect) -> bool,
    {
        self.alive(root)?;
        struct StackEntry {
            node: NodeId,
            offset_x: f32,
            offset_y: f32,
            // `display:none` on an ancestor: taffy stops laying out the subtree, leaving stale layouts,
            // so descendants keep their last visible size and widgets that draw at fixed coordinates
            // (e.g. a Canvas) would still paint. Force the whole subtree to a zero size instead.
            hidden: bool,
        }

        let mut stack = Vec::with_capacity(64);
        stack.push(StackEntry {
            node: root,
            offset_x: 0.0,
            offset_y: 0.0,
            hidden: false,
        });

        while let Some(entry) = stack.pop() {
            let layout = self.tree.layout(entry.node).map_err(LayoutError::from)?;
            let abs_x = entry.offset_x + layout.location.x;
            let abs_y = entry.offset_y + layout.location.y;
            let hidden = entry.hidden
                || self
                    .tree
                    .style(entry.node)
                    .map(|s| s.display == taffy::Display::None)
                    .unwrap_or(false);
            let (w, h) = if hidden {
                (0.0, 0.0)
            } else {
                (layout.size.width, layout.size.height)
            };

            let descend = f(entry.node, geometry_core::Rect::new(abs_x, abs_y, w, h));

            if descend {
                let base = stack.len();
                for child in self.tree.child_ids(entry.node) {
                    stack.push(StackEntry {
                        node: child,
                        offset_x: abs_x,
                        offset_y: abs_y,
                        hidden,
                    });
                }
                stack[base..].reverse();
            }
        }
        Ok(())
    }
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lay_out(engine: &mut LayoutEngine, root: NodeId) {
        engine
            .compute_layout(
                root,
                AvailableSpace::Definite(300.0),
                AvailableSpace::Definite(100.0),
            )
            .unwrap();
    }

    /// Two owners can reach the same node without seeing each other — a list reconciling away a row it no
    /// longer has, and that row's own owner being disposed — and both are right to free what they held.
    /// Taffy panics on a key it has already handed back, so the second free took the process down with it.
    #[test]
    fn freeing_a_node_twice_is_a_no_op() {
        let mut engine = LayoutEngine::new();
        let node = engine.new_leaf(LayoutStyle::new()).unwrap();
        engine.remove(node);
        engine.remove(node);
        assert!(engine.mark_dirty(node).is_err(), "and it stays gone");
    }

    /// Every mutator answers for a freed node instead of taking the process down with it.
    ///
    /// Taffy indexes its slot map directly — `style()` included, so there is nothing to guard with but our
    /// own record — and a widget can outlive its node by design: a `Segment` holds it so a re-render
    /// mid-dispatch can still flatten it, so an effect it owns may fire once after the node is gone. These
    /// all return `Result`; an error is what the signature already promised.
    #[test]
    fn a_freed_node_is_an_error_and_never_a_panic() {
        let mut engine = LayoutEngine::new();
        let parent = engine.new_container(LayoutStyle::new(), &[]).unwrap();
        let child = engine.new_leaf(LayoutStyle::new()).unwrap();
        let ghost = engine.new_leaf(LayoutStyle::new()).unwrap();
        engine.remove(ghost);

        assert!(engine.set_children(ghost, &[]).is_err());
        assert!(engine.set_children(parent, &[ghost]).is_err());
        assert!(engine.set_style(ghost, LayoutStyle::new()).is_err());
        assert!(engine.mark_dirty(ghost).is_err());
        assert!(engine.add_child(parent, ghost).is_err());
        assert!(engine.remove_child(parent, ghost).is_err());
        assert!(engine.layout(ghost).is_err());
        assert!(
            engine
                .compute_layout(
                    ghost,
                    AvailableSpace::MaxContent,
                    AvailableSpace::MaxContent
                )
                .is_err()
        );
        assert_eq!(engine.is_size_auto(ghost), (false, false));
        assert!(engine.is_fixed_size(ghost).is_none());
        // The reads that cannot fail still have to answer without touching taffy.
        engine.set_width(ghost, Some(10.0));
        engine.set_height(ghost, None);
        engine.set_leading_margin(ghost, true, 4.0);

        // And the live node beside it is untouched by any of that.
        assert!(engine.set_children(parent, &[child]).is_ok());
    }

    #[test]
    fn flipping_direction_relays_an_existing_row_without_rebuilding_it() {
        // The whole point of resolving late: the same nodes, laid out the other way round.
        let mut engine = LayoutEngine::new();
        let first = engine
            .new_leaf(LayoutStyle::new().width(50.0).height(10.0))
            .unwrap();
        let second = engine
            .new_leaf(LayoutStyle::new().width(50.0).height(10.0))
            .unwrap();
        let row = engine
            .new_container(
                LayoutStyle::new().flex_row().width(300.0).height(100.0),
                &[first, second],
            )
            .unwrap();
        lay_out(&mut engine, row);
        assert_eq!(engine.layout(first).unwrap().x, 0.0);
        assert_eq!(engine.layout(second).unwrap().x, 50.0);

        assert!(engine.set_direction(Direction::Rtl));
        engine.mark_dirty(row).unwrap();
        lay_out(&mut engine, row);
        assert_eq!(
            engine.layout(first).unwrap().x,
            250.0,
            "the first item now starts at the right edge"
        );
        assert_eq!(engine.layout(second).unwrap().x, 200.0);
    }

    #[test]
    fn flipping_direction_moves_logical_padding_to_the_other_edge() {
        let mut engine = LayoutEngine::new();
        let child = engine
            .new_leaf(LayoutStyle::new().width(50.0).height(10.0))
            .unwrap();
        let box_ = engine
            .new_container(
                LayoutStyle::new()
                    .flex_column()
                    .width(300.0)
                    .height(100.0)
                    .padding_start(20.0),
                &[child],
            )
            .unwrap();
        lay_out(&mut engine, box_);
        assert_eq!(engine.layout(child).unwrap().x, 20.0);

        engine.set_direction(Direction::Rtl);
        engine.mark_dirty(box_).unwrap();
        lay_out(&mut engine, box_);
        assert_eq!(
            engine.layout(child).unwrap().x,
            0.0,
            "padding moved to the right edge, so the child starts flush left"
        );
    }

    #[test]
    fn setting_the_same_direction_reports_no_change() {
        let mut engine = LayoutEngine::new();
        assert!(!engine.set_direction(Direction::Ltr));
        assert!(engine.set_direction(Direction::Rtl));
        assert!(!engine.set_direction(Direction::Rtl));
    }

    #[test]
    fn restyling_a_node_drops_the_logical_edges_it_no_longer_has() {
        // A stale entry would keep re-applying the old padding on every flip.
        let mut engine = LayoutEngine::new();
        let node = engine
            .new_leaf(LayoutStyle::new().padding_start(20.0).width(50.0))
            .unwrap();
        engine
            .set_style(node, LayoutStyle::new().width(50.0))
            .unwrap();
        engine.set_direction(Direction::Rtl);
        let style = engine.tree.style(node).unwrap();
        assert_eq!(style.padding.left, taffy::LengthPercentage::length(0.0));
        assert_eq!(style.padding.right, taffy::LengthPercentage::length(0.0));
    }

    #[test]
    fn a_gap_margin_follows_the_edge_its_row_leads_from() {
        let mut engine = LayoutEngine::new();
        let first = engine.new_leaf(LayoutStyle::new().width(50.0)).unwrap();
        let second = engine.new_leaf(LayoutStyle::new().width(50.0)).unwrap();
        let row = engine
            .new_container(LayoutStyle::new().flex_row().width(300.0), &[first, second])
            .unwrap();
        engine.set_leading_margin(second, true, 8.0);
        assert_eq!(
            engine.tree.style(second).unwrap().margin.left,
            taffy::LengthPercentageAuto::length(8.0)
        );

        engine.set_direction(Direction::Rtl);
        engine.mark_dirty(row).unwrap();
        let margin = engine.tree.style(second).unwrap().margin;
        assert_eq!(
            margin.right,
            taffy::LengthPercentageAuto::length(8.0),
            "the gap moved to the edge the reversed row leads from"
        );
        assert_eq!(
            margin.left,
            taffy::LengthPercentageAuto::length(0.0),
            "and does not linger on the old one"
        );
    }

    #[test]
    fn engine_leaf_layout() {
        let mut engine = LayoutEngine::new();
        let leaf = engine
            .new_leaf(LayoutStyle::new().width(50.0).height(40.0))
            .unwrap();
        engine
            .compute_layout(
                leaf,
                AvailableSpace::Definite(200.0),
                AvailableSpace::Definite(200.0),
            )
            .unwrap();
        let rect = engine.layout(leaf).unwrap();
        assert_eq!(rect.width, 50.0_f32);
        assert_eq!(rect.height, 40.0_f32);
    }

    #[test]
    fn engine_flex_row_positions() {
        let mut engine = LayoutEngine::new();
        let child1 = engine
            .new_leaf(LayoutStyle::new().width(100.0).height(100.0))
            .unwrap();
        let child2 = engine
            .new_leaf(LayoutStyle::new().width(100.0).height(100.0))
            .unwrap();
        let root = engine
            .new_container(
                LayoutStyle::new().flex_row().width(200.0).height(100.0),
                &[child1, child2],
            )
            .unwrap();
        engine
            .compute_layout(
                root,
                AvailableSpace::Definite(200.0),
                AvailableSpace::Definite(100.0),
            )
            .unwrap();

        let r1 = engine.layout(child1).unwrap();
        let r2 = engine.layout(child2).unwrap();
        assert_eq!(r1.x, 0.0_f32);
        assert_eq!(r1.y, 0.0_f32);
        assert_eq!(r2.x, 100.0_f32);
        assert_eq!(r2.y, 0.0_f32);
    }

    #[test]
    fn engine_flex_column_positions() {
        let mut engine = LayoutEngine::new();
        let child1 = engine
            .new_leaf(LayoutStyle::new().width(100.0).height(100.0))
            .unwrap();
        let child2 = engine
            .new_leaf(LayoutStyle::new().width(100.0).height(100.0))
            .unwrap();
        let root = engine
            .new_container(
                LayoutStyle::new().flex_column().width(100.0).height(200.0),
                &[child1, child2],
            )
            .unwrap();
        engine
            .compute_layout(
                root,
                AvailableSpace::Definite(100.0),
                AvailableSpace::Definite(200.0),
            )
            .unwrap();

        let r1 = engine.layout(child1).unwrap();
        let r2 = engine.layout(child2).unwrap();
        assert_eq!(r1.x, 0.0_f32);
        assert_eq!(r1.y, 0.0_f32);
        assert_eq!(r2.x, 0.0_f32);
        assert_eq!(r2.y, 100.0_f32);
    }

    #[test]
    fn engine_walk_absolute() {
        let mut engine = LayoutEngine::new();
        let inner_child = engine
            .new_leaf(LayoutStyle::new().width(50.0).height(50.0))
            .unwrap();
        let inner = engine
            .new_container(
                LayoutStyle::new().flex_row().width(50.0).height(50.0),
                &[inner_child],
            )
            .unwrap();
        let outer_first = engine
            .new_leaf(LayoutStyle::new().width(100.0).height(50.0))
            .unwrap();
        let root = engine
            .new_container(
                LayoutStyle::new().flex_row().width(150.0).height(50.0),
                &[outer_first, inner],
            )
            .unwrap();
        engine
            .compute_layout(
                root,
                AvailableSpace::Definite(150.0),
                AvailableSpace::Definite(50.0),
            )
            .unwrap();

        let mut hits: Vec<(NodeId, geometry_core::Rect)> = Vec::new();
        engine
            .walk(root, &mut |node, rect| {
                hits.push((node, rect));
                true
            })
            .unwrap();

        let inner_child_rect = hits
            .iter()
            .find(|(n, _)| *n == inner_child)
            .map(|(_, r)| *r)
            .unwrap();
        assert_eq!(inner_child_rect.x, 100.0_f32);
        assert_eq!(inner_child_rect.y, 0.0_f32);
        assert_eq!(inner_child_rect.width, 50.0_f32);
        assert_eq!(inner_child_rect.height, 50.0_f32);
    }

    #[test]
    fn engine_set_style() {
        let mut engine = LayoutEngine::new();
        let leaf = engine
            .new_leaf(LayoutStyle::new().width(10.0).height(10.0))
            .unwrap();
        engine
            .set_style(leaf, LayoutStyle::new().width(80.0).height(60.0))
            .unwrap();
        engine
            .compute_layout(
                leaf,
                AvailableSpace::Definite(200.0),
                AvailableSpace::Definite(200.0),
            )
            .unwrap();
        let rect = engine.layout(leaf).unwrap();
        assert_eq!(rect.width, 80.0_f32);
        assert_eq!(rect.height, 60.0_f32);
    }
}
