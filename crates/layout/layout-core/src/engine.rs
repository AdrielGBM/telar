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
    /// Nodes whose style holds logical edges, kept so a direction flip can re-resolve them from the original
    /// intent. Only these nodes pay for the copy; a tree with no logical edges keeps the map empty.
    logical: FxHashMap<NodeId, LayoutStyle>,
    /// Rows whose main axis follows the writing direction. Held apart from `logical` because flipping one is
    /// a flag toggle that needs no original style — and rows are common enough that a `NodeId` is worth the
    /// saving over a whole `LayoutStyle`.
    directional_rows: FxHashSet<NodeId>,
    /// Main-axis gap margins applied by [`set_leading_margin`](Self::set_leading_margin), which sit on the
    /// *leading* edge and so must move to the other side when a row reverses.
    leading_margins: FxHashMap<NodeId, (bool, f32)>,
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            tree: TaffyTree::new(),
            direction: Direction::default(),
            logical: FxHashMap::default(),
            directional_rows: FxHashSet::default(),
            leading_margins: FxHashMap::default(),
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
            if let Ok(current) = self.tree.style(node) {
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
        let logical = std::mem::take(&mut self.logical);
        for (&node, style) in &logical {
            let _ = self.tree.set_style(node, style.resolve(direction));
        }
        self.logical = logical;
        let margins = std::mem::take(&mut self.leading_margins);
        for (&node, &(is_row, px)) in &margins {
            self.apply_leading_margin(node, is_row, px, true);
        }
        self.leading_margins = margins;
        true
    }

    /// Records whichever direction-dependent parts of `style` the node will need re-resolved on a flip, and
    /// drops any it no longer has — a restyled node must not keep the previous style's logical edges.
    fn track(&mut self, node: NodeId, style: LayoutStyle) {
        if style.logical.has_edges() {
            self.directional_rows.remove(&node);
            self.logical.insert(node, style);
            return;
        }
        self.logical.remove(&node);
        if style.logical.row_follows_direction {
            self.directional_rows.insert(node);
        } else {
            self.directional_rows.remove(&node);
        }
    }

    fn forget(&mut self, node: NodeId) {
        self.logical.remove(&node);
        self.directional_rows.remove(&node);
        self.leading_margins.remove(&node);
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

    pub fn set_style(&mut self, node: NodeId, style: LayoutStyle) -> Result<(), LayoutError> {
        self.tree.set_style(node, style.resolve(self.direction))?;
        self.track(node, style);
        Ok(())
    }

    /// Replaces `parent`'s children with `children`, in order. Used by reactive lists to insert, move,
    /// and drop item nodes as their source collection changes.
    pub fn set_children(&mut self, parent: NodeId, children: &[NodeId]) -> Result<(), LayoutError> {
        self.tree
            .set_children(parent, children)
            .map_err(LayoutError::from)
    }

    /// Appends `child` to `parent`'s existing children (unlike [`set_children`], which replaces them). Used
    /// to attach an overlay's out-of-flow content to the layout root without touching the root's other children.
    pub fn add_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), LayoutError> {
        self.tree
            .add_child(parent, child)
            .map_err(LayoutError::from)
    }

    /// Detaches `child` from `parent` (does not free it — call [`remove`] afterwards to release the node).
    pub fn remove_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), LayoutError> {
        self.tree
            .remove_child(parent, child)
            .map(|_| ())
            .map_err(LayoutError::from)
    }

    /// Frees a node (and its measure context) from the tree. The caller must have already detached it from
    /// its parent (via [`set_children`]); a removed node id must not be used again.
    pub fn remove(&mut self, node: NodeId) {
        self.forget(node);
        let _ = self.tree.remove(node);
    }

    pub fn mark_dirty(&mut self, node: NodeId) -> Result<(), LayoutError> {
        self.tree.mark_dirty(node).map_err(LayoutError::from)
    }

    /// Whether the node's `width`/`height` are `auto` (i.e. content-sized).
    pub fn is_size_auto(&self, node: NodeId) -> (bool, bool) {
        match self.tree.style(node) {
            Ok(s) => (s.size.width.is_auto(), s.size.height.is_auto()),
            Err(_) => (false, false),
        }
    }

    /// Sets the node's width to a definite length, or back to `auto` when `None`.
    pub fn set_width(&mut self, node: NodeId, width: Option<f32>) {
        if let Ok(s) = self.tree.style(node) {
            let mut style = s.clone();
            style.size.width = width.map_or(taffy::Dimension::auto(), taffy::Dimension::length);
            let _ = self.tree.set_style(node, style);
        }
    }

    /// Sets the node's height to a definite length, or back to `auto` when `None`.
    pub fn set_height(&mut self, node: NodeId, height: Option<f32>) {
        if let Ok(s) = self.tree.style(node) {
            let mut style = s.clone();
            style.size.height = height.map_or(taffy::Dimension::auto(), taffy::Dimension::length);
            let _ = self.tree.set_style(node, style);
        }
    }

    /// Sets the node's minimum height to a definite length, or clears it (`auto`) when `None`. Lets a
    /// content-measured leaf (e.g. a code editor's text area) fill a viewport it would otherwise underflow.
    pub fn set_min_height(&mut self, node: NodeId, height: Option<f32>) {
        if let Ok(s) = self.tree.style(node) {
            let mut style = s.clone();
            style.min_size.height =
                height.map_or(taffy::Dimension::auto(), taffy::Dimension::length);
            let _ = self.tree.set_style(node, style);
        }
    }

    /// Whether the node lays its children along the main (horizontal) axis — a flex row. A column, or any
    /// non-row node (missing / errored), is `false`. A transparent fragment reads its host's axis to know
    /// which margin edge a per-item gap sits on.
    pub fn is_row(&self, node: NodeId) -> bool {
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
        self.leading_margins.insert(node, (is_row, px));
        self.apply_leading_margin(node, is_row, px, false);
    }

    /// Whether the node's host lays its children out from the right — an RTL row, or one explicitly reversed.
    fn leads_from_right(&self, node: NodeId) -> bool {
        self.tree
            .parent(node)
            .and_then(|parent| self.tree.style(parent).ok())
            .map(|s| s.flex_direction == taffy::FlexDirection::RowReverse)
            .unwrap_or(false)
    }

    /// `clear_opposite` un-sets the horizontal edge this node's gap used to sit on, which a re-application
    /// after a direction flip needs and a first application must not do (it would clobber an author's margin).
    fn apply_leading_margin(&mut self, node: NodeId, is_row: bool, px: f32, clear_opposite: bool) {
        let leading_right = is_row && self.leads_from_right(node);
        let Ok(current) = self.tree.style(node) else {
            return;
        };
        let mut style = current.clone();
        let m = taffy::LengthPercentageAuto::length(px);
        if is_row {
            if leading_right {
                style.margin.right = m;
            } else {
                style.margin.left = m;
            }
            if clear_opposite {
                let zero = taffy::LengthPercentageAuto::length(0.0);
                if leading_right {
                    style.margin.left = zero;
                } else {
                    style.margin.right = zero;
                }
            }
        } else {
            style.margin.top = m;
        }
        let _ = self.tree.set_style(node, style);
    }

    /// Toggles a node in or out of layout flow. A hidden node (`Display::None`) takes no space and lays out none of its subtree; a visible node is `Display::Flex`. Used for responsive show/hide (e.g. collapsing a sidebar on narrow windows).
    pub fn set_display(&mut self, node: NodeId, visible: bool) {
        if let Ok(s) = self.tree.style(node) {
            let mut style = s.clone();
            style.display = if visible {
                taffy::Display::Flex
            } else {
                taffy::Display::None
            };
            let _ = self.tree.set_style(node, style);
        }
    }

    pub fn compute_layout(
        &mut self,
        root: NodeId,
        available_width: AvailableSpace,
        available_height: AvailableSpace,
    ) -> Result<(), LayoutError> {
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
        self.tree.dirty(node).unwrap_or(true)
    }

    pub fn layout(&self, node: NodeId) -> Result<geometry_core::Rect, LayoutError> {
        let layout = self.tree.layout(node).map_err(LayoutError::from)?;
        Ok(geometry_core::Rect::new(
            layout.location.x,
            layout.location.y,
            layout.size.width,
            layout.size.height,
        ))
    }

    pub fn is_fixed_size(&self, node: NodeId) -> Option<(f32, f32)> {
        let style = self.tree.style(node).ok()?;
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
