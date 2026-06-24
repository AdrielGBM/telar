use taffy::{TaffyTree, TraversePartialTree};

use crate::error::LayoutError;
use crate::style::{AvailableSpace, LayoutStyle};

pub type NodeId = taffy::NodeId;

/// Per-node measure callback: given the available main-axis width, returns the
/// node's intrinsic (width, height). Used for text nodes whose height depends on
/// how many lines the content wraps into at the resolved width.
pub type MeasureFn = Box<dyn FnMut(f32) -> (f32, f32)>;

pub struct LayoutEngine {
    tree: TaffyTree<MeasureFn>,
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            tree: TaffyTree::new(),
        }
    }

    pub fn new_leaf(&mut self, style: LayoutStyle) -> Result<NodeId, LayoutError> {
        self.tree.new_leaf(style.inner).map_err(LayoutError::from)
    }

    pub fn new_measured_leaf(
        &mut self,
        style: LayoutStyle,
        measure: MeasureFn,
    ) -> Result<NodeId, LayoutError> {
        self.tree
            .new_leaf_with_context(style.inner, measure)
            .map_err(LayoutError::from)
    }

    pub fn new_container(
        &mut self,
        style: LayoutStyle,
        children: &[NodeId],
    ) -> Result<NodeId, LayoutError> {
        self.tree
            .new_with_children(style.inner, children)
            .map_err(LayoutError::from)
    }

    pub fn set_style(&mut self, node: NodeId, style: LayoutStyle) -> Result<(), LayoutError> {
        self.tree
            .set_style(node, style.inner)
            .map_err(LayoutError::from)
    }

    pub fn mark_dirty(&mut self, node: NodeId) -> Result<(), LayoutError> {
        self.tree.mark_dirty(node).map_err(LayoutError::from)
    }

    /// Whether the node's `width`/`height` are `auto` (i.e. content-sized).
    pub fn size_is_auto(&self, node: NodeId) -> (bool, bool) {
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
                    // Width to wrap against: a resolved width wins, else the definite
                    // available width, else a large bound so MaxContent stays single-line.
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

    pub fn get_layout(&self, node: NodeId) -> Result<geometry_core::Rect, LayoutError> {
        let layout = self.tree.layout(node).map_err(LayoutError::from)?;
        Ok(geometry_core::Rect::new(
            layout.location.x,
            layout.location.y,
            layout.size.width,
            layout.size.height,
        ))
    }

    pub fn is_fixed_size_node(&self, node: NodeId) -> Option<(f32, f32)> {
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
        }

        let mut stack = Vec::with_capacity(64);
        stack.push(StackEntry {
            node: root,
            offset_x: 0.0,
            offset_y: 0.0,
        });

        while let Some(entry) = stack.pop() {
            let layout = self.tree.layout(entry.node).map_err(LayoutError::from)?;
            let abs_x = entry.offset_x + layout.location.x;
            let abs_y = entry.offset_y + layout.location.y;

            let descend = f(
                entry.node,
                geometry_core::Rect::new(abs_x, abs_y, layout.size.width, layout.size.height),
            );

            if descend {
                let base = stack.len();
                for child in self.tree.child_ids(entry.node) {
                    stack.push(StackEntry {
                        node: child,
                        offset_x: abs_x,
                        offset_y: abs_y,
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
        let rect = engine.get_layout(leaf).unwrap();
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

        let r1 = engine.get_layout(child1).unwrap();
        let r2 = engine.get_layout(child2).unwrap();
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

        let r1 = engine.get_layout(child1).unwrap();
        let r2 = engine.get_layout(child2).unwrap();
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
        let rect = engine.get_layout(leaf).unwrap();
        assert_eq!(rect.width, 80.0_f32);
        assert_eq!(rect.height, 60.0_f32);
    }
}
