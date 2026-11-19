//! The node storage taffy lays out, which also knows what each layout pass moved.
//!
//! Taffy's own tree rounds every node of the root it computed and says nothing of what changed, so a pass that moved one box cost as much as the page. This one records each node whose layout taffy wrote a different value for, rounds only the subtrees holding something new, and stamps what that rounding moved, for a walk to visit those subtrees alone.

use std::cell::Cell;

use slotmap::{DefaultKey, SecondaryMap, SlotMap};
use taffy::{
    BlockContext, Cache, CacheTree, ClearState, Display, Layout, LayoutBlockContainer,
    LayoutFlexboxContainer, LayoutGridContainer, LayoutInput, LayoutOutput, LayoutPartialTree,
    NodeId, RunMode, Style, TraversePartialTree, TraverseTree, compute_block_layout,
    compute_cached_layout, compute_flexbox_layout, compute_grid_layout, compute_hidden_layout,
    compute_root_layout,
};

use crate::engine::MeasureFn;

/// Where a walk last placed a node from: its parent's position, whether it was under a hidden box, and whether it was displaced as a sticky one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlacedFrom {
    pub(crate) origin: (f32, f32),
    pub(crate) hidden: bool,
    pub(crate) sticky: bool,
}

struct Node {
    style: Style,
    children: Vec<NodeId>,
    parent: Option<NodeId>,
    cache: Cache,
    unrounded: Layout,
    rounded: Layout,
    has_measure: bool,
    /// The content of a scroll container: laid out inside it, but reaching nothing on its behalf.
    scroll_content: bool,
    /// How far right and down the boxes under it reach, rounded, from its own top-left: every box but those of a hidden subtree or a scroll container's content, which reach only as far as the container.
    reach: (f32, f32),
    /// Its unrounded layout, or that of something under it, or the list of its children, changed since it was last rounded. Every ancestor of a stale node is stale too.
    stale: bool,
    /// The unrounded position of its parent when it was last rounded, `None` before it ever was: rounding depends on it as much as on the node's own layout.
    rounded_from: Option<(f32, f32)>,
    /// The rounding pass that last changed its rounded layout or that of anything under it, or rounded it for the first time.
    moved_in: u64,
    // Cells, because a walk reads the tree and records where it placed each node as it goes.
    placed_from: Cell<Option<PlacedFrom>>,
    placed_in: Cell<u64>,
}

impl Node {
    fn new(style: Style) -> Self {
        Self {
            style,
            children: Vec::new(),
            parent: None,
            cache: Cache::new(),
            unrounded: Layout::new(),
            rounded: Layout::new(),
            has_measure: false,
            scroll_content: false,
            reach: (0.0, 0.0),
            stale: true,
            rounded_from: None,
            moved_in: 0,
            placed_from: Cell::new(None),
            placed_in: Cell::new(0),
        }
    }
}

/// The layout tree's nodes: their styles, children, measure hooks, and both the layout taffy computed and the whole-pixel one drawn from it.
pub(crate) struct LayoutTree {
    nodes: SlotMap<DefaultKey, Node>,
    measures: SecondaryMap<DefaultKey, MeasureFn>,
    /// How many rounding passes have run, which is what [`moved_in`](Self::moved_in) is counted in.
    rounding_passes: u64,
}

impl LayoutTree {
    pub(crate) fn new() -> Self {
        Self {
            nodes: SlotMap::new(),
            measures: SecondaryMap::new(),
            rounding_passes: 0,
        }
    }

    fn node(&self, node: NodeId) -> &Node {
        &self.nodes[DefaultKey::from(node)]
    }

    fn node_mut(&mut self, node: NodeId) -> &mut Node {
        &mut self.nodes[DefaultKey::from(node)]
    }

    pub(crate) fn new_leaf(&mut self, style: Style) -> NodeId {
        self.nodes.insert(Node::new(style)).into()
    }

    pub(crate) fn new_measured_leaf(&mut self, style: Style, measure: MeasureFn) -> NodeId {
        let key = self.nodes.insert(Node {
            has_measure: true,
            ..Node::new(style)
        });
        self.measures.insert(key, measure);
        key.into()
    }

    pub(crate) fn new_with_children(&mut self, style: Style, children: &[NodeId]) -> NodeId {
        let node: NodeId = self.nodes.insert(Node::new(style)).into();
        for &child in children {
            self.node_mut(child).parent = Some(node);
        }
        self.node_mut(node).children = children.to_vec();
        node
    }

    /// Frees `node` and its measure hook, taking it out of its parent's children and leaving its own children without a parent.
    pub(crate) fn remove(&mut self, node: NodeId) {
        if let Some(parent) = self.node(node).parent {
            self.node_mut(parent)
                .children
                .retain(|&child| child != node);
            self.reshaped(parent);
        }
        let key = DefaultKey::from(node);
        for child in std::mem::take(&mut self.nodes[key].children) {
            self.node_mut(child).parent = None;
        }
        self.measures.remove(key);
        self.nodes.remove(key);
    }

    pub(crate) fn add_child(&mut self, parent: NodeId, child: NodeId) {
        self.node_mut(child).parent = Some(parent);
        self.node_mut(parent).children.push(child);
        self.reshaped(parent);
    }

    /// Replaces `parent`'s children, taking each out of whichever other node held it.
    pub(crate) fn set_children(&mut self, parent: NodeId, children: &[NodeId]) {
        for child in std::mem::take(&mut self.node_mut(parent).children) {
            self.node_mut(child).parent = None;
        }
        for &child in children {
            if let Some(previous) = self.node(child).parent {
                self.remove_child(previous, child);
            }
            self.node_mut(child).parent = Some(parent);
        }
        self.node_mut(parent).children = children.to_vec();
        self.reshaped(parent);
    }

    /// Takes `child` out of `parent`'s children, if it is among them.
    pub(crate) fn remove_child(&mut self, parent: NodeId, child: NodeId) {
        let children = &mut self.node_mut(parent).children;
        let Some(index) = children.iter().position(|&at| at == child) else {
            return;
        };
        children.remove(index);
        self.node_mut(child).parent = None;
        self.reshaped(parent);
    }

    pub(crate) fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.node(node).parent
    }

    pub(crate) fn children(&self, node: NodeId) -> &[NodeId] {
        &self.node(node).children
    }

    pub(crate) fn style(&self, node: NodeId) -> &Style {
        &self.node(node).style
    }

    pub(crate) fn set_style(&mut self, node: NodeId, style: Style) {
        self.node_mut(node).style = style;
        self.mark_dirty(node);
        // Whether it is shown decides how far its parent reaches, whatever layout makes of it.
        self.mark_stale(node);
    }

    /// Marks `node` as the content of a scroll container, before it is hung from one.
    pub(crate) fn mark_scroll_content(&mut self, node: NodeId) {
        self.node_mut(node).scroll_content = true;
    }

    /// How far right and down the boxes under `node` reach from its top-left, as of the last rounding. See [`Node::reach`].
    pub(crate) fn reach(&self, node: NodeId) -> (f32, f32) {
        self.node(node).reach
    }

    pub(crate) fn has_measure(&self, node: NodeId) -> bool {
        self.node(node).has_measure
    }

    /// The layout as drawn: every edge on a whole pixel of the root it was computed from.
    pub(crate) fn layout(&self, node: NodeId) -> &Layout {
        &self.node(node).rounded
    }

    /// The layout as computed, before rounding.
    pub(crate) fn unrounded_layout(&self, node: NodeId) -> &Layout {
        &self.node(node).unrounded
    }

    /// Clears the cached layout of `node` and every ancestor, so the next pass lays them out again.
    pub(crate) fn mark_dirty(&mut self, node: NodeId) {
        let mut at = Some(node);
        while let Some(node) = at {
            let data = self.node_mut(node);
            if matches!(data.cache.clear(), ClearState::AlreadyEmpty) {
                return;
            }
            at = data.parent;
        }
    }

    pub(crate) fn is_dirty(&self, node: NodeId) -> bool {
        self.node(node).cache.is_empty()
    }

    /// Lays `root` out in `available_space` and rounds what that moved.
    pub(crate) fn compute(
        &mut self,
        root: NodeId,
        available_space: taffy::Size<taffy::AvailableSpace>,
        measure: impl FnMut(LayoutInput, Option<&mut MeasureFn>, &Style) -> LayoutOutput,
    ) {
        compute_root_layout(
            &mut Pass {
                tree: self,
                measure,
            },
            root,
            available_space,
        );
        self.rounding_passes += 1;
        self.round(root, (0.0, 0.0));
    }

    /// The rounding pass the last [`compute`](Self::compute) ran, which [`moved_in`](Self::moved_in) is compared with.
    pub(crate) fn rounding_passes(&self) -> u64 {
        self.rounding_passes
    }

    /// The rounding pass that last moved `node` or something under it. A walk that last placed it before that pass has to place this subtree again; one after can leave it where it is, as long as it is placed from where it was.
    pub(crate) fn moved_in(&self, node: NodeId) -> u64 {
        self.node(node).moved_in
    }

    pub(crate) fn placed_from(&self, node: NodeId) -> Option<PlacedFrom> {
        self.node(node).placed_from.get()
    }

    pub(crate) fn placed_in(&self, node: NodeId) -> u64 {
        self.node(node).placed_in.get()
    }

    pub(crate) fn note_placed(&self, node: NodeId, from: PlacedFrom, walk: u64) {
        let data = self.node(node);
        data.placed_from.set(Some(from));
        data.placed_in.set(walk);
    }

    /// Marks `node` and its ancestors as holding something rounding has not seen yet.
    fn mark_stale(&mut self, node: NodeId) {
        let mut at = Some(node);
        while let Some(node) = at {
            let data = self.node_mut(node);
            if data.stale {
                return;
            }
            data.stale = true;
            at = data.parent;
        }
    }

    /// A change to `parent`'s children: laid out again, and rounded again so that a child added at the origin with no size is still reached.
    fn reshaped(&mut self, parent: NodeId) {
        self.mark_dirty(parent);
        self.mark_stale(parent);
    }

    /// Rounds `node`, placed under a parent standing at `from` before rounding, and the children that are stale or whose parent moved; returns whether anything it rounded moved, stamping each such node with this pass.
    ///
    /// Each edge is rounded where it falls in the root, not where it falls in its parent, so the gaps between boxes stay whole pixels however their fractions add up: the rule taffy's own rounding follows, value for value.
    fn round(&mut self, node: NodeId, from: (f32, f32)) -> bool {
        let data = self.node_mut(node);
        let unrounded = data.unrounded;
        let at = (from.0 + unrounded.location.x, from.1 + unrounded.location.y);
        let rounded = rounded(&unrounded, at);
        let mut moved = data.rounded_from.is_none() || data.rounded != rounded;
        data.rounded = rounded;
        data.rounded_from = Some(from);
        data.stale = false;
        let mut reach = (0.0f32, 0.0f32);
        for index in 0..self.node(node).children.len() {
            let child = self.node(node).children[index];
            let child_data = self.node(child);
            if child_data.stale || child_data.rounded_from != Some(at) {
                moved |= self.round(child, at);
            }
            let child_data = self.node(child);
            if child_data.style.display == Display::None || child_data.scroll_content {
                continue;
            }
            let (location, size) = (child_data.rounded.location, child_data.rounded.size);
            reach.0 = reach.0.max(location.x + size.width.max(child_data.reach.0));
            reach.1 = reach
                .1
                .max(location.y + size.height.max(child_data.reach.1));
        }
        self.node_mut(node).reach = reach;
        if moved {
            self.node_mut(node).moved_in = self.rounding_passes;
        }
        moved
    }
}

/// `unrounded` with every edge on a whole pixel of the root, for a box whose top-left stands at `at` there before rounding.
fn rounded(unrounded: &Layout, at: (f32, f32)) -> Layout {
    let (x, y) = at;
    let size = unrounded.size;
    let mut layout = *unrounded;
    layout.location.x = unrounded.location.x.round();
    layout.location.y = unrounded.location.y.round();
    layout.size.width = (x + size.width).round() - x.round();
    layout.size.height = (y + size.height).round() - y.round();
    layout.scrollbar_size.width = unrounded.scrollbar_size.width.round();
    layout.scrollbar_size.height = unrounded.scrollbar_size.height.round();
    layout.border.left = (x + unrounded.border.left).round() - x.round();
    layout.border.right =
        (x + size.width).round() - (x + size.width - unrounded.border.right).round();
    layout.border.top = (y + unrounded.border.top).round() - y.round();
    layout.border.bottom =
        (y + size.height).round() - (y + size.height - unrounded.border.bottom).round();
    layout.padding.left = (x + unrounded.padding.left).round() - x.round();
    layout.padding.right =
        (x + size.width).round() - (x + size.width - unrounded.padding.right).round();
    layout.padding.top = (y + unrounded.padding.top).round() - y.round();
    layout.padding.bottom =
        (y + size.height).round() - (y + size.height - unrounded.padding.bottom).round();
    let overflow = unrounded.scrollable_overflow_rect;
    layout.scrollable_overflow_rect.left = (x + overflow.left).round() - x.round();
    layout.scrollable_overflow_rect.right = (x + overflow.right).round() - x.round();
    layout.scrollable_overflow_rect.top = (y + overflow.top).round() - y.round();
    layout.scrollable_overflow_rect.bottom = (y + overflow.bottom).round() - y.round();
    layout
}

/// The tree as taffy's algorithms see it for one pass, with the hook that measures a leaf.
struct Pass<'t, M> {
    tree: &'t mut LayoutTree,
    measure: M,
}

impl<M> Pass<'_, M>
where
    M: FnMut(LayoutInput, Option<&mut MeasureFn>, &Style) -> LayoutOutput,
{
    fn lay_out(
        &mut self,
        node: NodeId,
        inputs: LayoutInput,
        block_ctx: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        if inputs.run_mode == RunMode::PerformHiddenLayout {
            return compute_hidden_layout(self, node);
        }
        compute_cached_layout(self, node, inputs, |pass, node, inputs| {
            let has_children = !pass.tree.node(node).children.is_empty();
            match (pass.tree.node(node).style.display, has_children) {
                (Display::None, _) => compute_hidden_layout(pass, node),
                (Display::Block, true) => compute_block_layout(pass, node, inputs, block_ctx),
                (Display::FlowRoot, true) => compute_block_layout(pass, node, inputs, None),
                (Display::Flex, true) => compute_flexbox_layout(pass, node, inputs),
                (Display::Grid, true) => compute_grid_layout(pass, node, inputs),
                (_, false) => {
                    let key = DefaultKey::from(node);
                    let tree = &mut *pass.tree;
                    let style = &tree.nodes[key].style;
                    let measure = if tree.nodes[key].has_measure {
                        tree.measures.get_mut(key)
                    } else {
                        None
                    };
                    (pass.measure)(inputs, measure, style)
                }
            }
        })
    }
}

impl<M> TraversePartialTree for Pass<'_, M> {
    type ChildIter<'a>
        = std::iter::Copied<std::slice::Iter<'a, NodeId>>
    where
        Self: 'a;

    fn child_ids(&self, parent: NodeId) -> Self::ChildIter<'_> {
        self.tree.node(parent).children.iter().copied()
    }

    fn child_count(&self, parent: NodeId) -> usize {
        self.tree.node(parent).children.len()
    }

    fn get_child_id(&self, parent: NodeId, index: usize) -> NodeId {
        self.tree.node(parent).children[index]
    }
}

impl<M> TraverseTree for Pass<'_, M> {}

impl<M> LayoutPartialTree for Pass<'_, M>
where
    M: FnMut(LayoutInput, Option<&mut MeasureFn>, &Style) -> LayoutOutput,
{
    type CoreContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    type CustomIdent = String;

    fn get_core_container_style(&self, node: NodeId) -> Self::CoreContainerStyle<'_> {
        &self.tree.node(node).style
    }

    fn set_unrounded_layout(&mut self, node: NodeId, layout: &Layout) {
        let data = self.tree.node_mut(node);
        if data.unrounded != *layout {
            data.unrounded = *layout;
            self.tree.mark_stale(node);
        }
    }

    fn compute_child_layout(&mut self, node: NodeId, inputs: LayoutInput) -> LayoutOutput {
        self.lay_out(node, inputs, None)
    }
}

impl<M> CacheTree for Pass<'_, M> {
    fn cache_get(&mut self, node: NodeId, input: &LayoutInput) -> Option<LayoutOutput> {
        self.tree.node_mut(node).cache.get(input)
    }

    fn cache_store(&mut self, node: NodeId, input: &LayoutInput, output: LayoutOutput) {
        self.tree.node_mut(node).cache.store(input, output)
    }

    fn cache_clear(&mut self, node: NodeId) {
        self.tree.node_mut(node).cache.clear();
    }
}

impl<M> LayoutBlockContainer for Pass<'_, M>
where
    M: FnMut(LayoutInput, Option<&mut MeasureFn>, &Style) -> LayoutOutput,
{
    type BlockContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    type BlockItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_block_container_style(&self, node: NodeId) -> Self::BlockContainerStyle<'_> {
        &self.tree.node(node).style
    }

    fn get_block_child_style(&self, child: NodeId) -> Self::BlockItemStyle<'_> {
        &self.tree.node(child).style
    }

    fn compute_block_child_layout(
        &mut self,
        node: NodeId,
        inputs: LayoutInput,
        block_ctx: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        self.lay_out(node, inputs, block_ctx)
    }
}

impl<M> LayoutFlexboxContainer for Pass<'_, M>
where
    M: FnMut(LayoutInput, Option<&mut MeasureFn>, &Style) -> LayoutOutput,
{
    type FlexboxContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    type FlexboxItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_flexbox_container_style(&self, node: NodeId) -> Self::FlexboxContainerStyle<'_> {
        &self.tree.node(node).style
    }

    fn get_flexbox_child_style(&self, child: NodeId) -> Self::FlexboxItemStyle<'_> {
        &self.tree.node(child).style
    }
}

impl<M> LayoutGridContainer for Pass<'_, M>
where
    M: FnMut(LayoutInput, Option<&mut MeasureFn>, &Style) -> LayoutOutput,
{
    type GridContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    type GridItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_grid_container_style(&self, node: NodeId) -> Self::GridContainerStyle<'_> {
        &self.tree.node(node).style
    }

    fn get_grid_child_style(&self, child: NodeId) -> Self::GridItemStyle<'_> {
        &self.tree.node(child).style
    }
}

#[cfg(test)]
#[path = "tree.test.rs"]
mod tests;
