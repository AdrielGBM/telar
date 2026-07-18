use std::cell::RefCell;
use std::sync::Arc;

use geometry_core::Rect;
use renderer_core::{
    BorderRadius, DrawCommand, PathData, PathStyle, RectStyle, TextRun, TextStyle,
};

thread_local! {
    static NODE_VEC_POOL: RefCell<Vec<Vec<RenderNode>>> = const { RefCell::new(Vec::new()) };
}

pub struct NodeVec(Vec<RenderNode>);

impl NodeVec {
    pub fn collect(iter: impl IntoIterator<Item = RenderNode>) -> Self {
        let mut v = NODE_VEC_POOL
            .with_borrow_mut(|pool| pool.pop())
            .unwrap_or_default();
        v.extend(iter);
        NodeVec(v)
    }
}

impl Drop for NodeVec {
    fn drop(&mut self) {
        let mut v = std::mem::take(&mut self.0);
        v.clear();
        NODE_VEC_POOL.with_borrow_mut(|pool| {
            if pool.len() < 32 {
                pool.push(v);
            }
        });
    }
}

impl std::ops::Deref for NodeVec {
    type Target = [RenderNode];
    fn deref(&self) -> &[RenderNode] {
        &self.0
    }
}

impl IntoIterator for NodeVec {
    type Item = RenderNode;
    type IntoIter = std::vec::IntoIter<RenderNode>;
    fn into_iter(self) -> Self::IntoIter {
        // ManuallyDrop suppresses NodeVec's Drop (which would return the vec to the pool) so we can move the inner vec out and iterate it instead.
        let mut md = std::mem::ManuallyDrop::new(self);
        let v = std::mem::take(&mut md.0);
        v.into_iter()
    }
}

pub enum RenderNode {
    Empty,
    Primitive(DrawCommand),
    Group {
        children: NodeVec,
    },
    Transform {
        matrix: [f32; 6],
        children: NodeVec,
    },
    Clip {
        rect: Rect,
        radius: BorderRadius,
        children: NodeVec,
    },
    Layer {
        opacity: f32,
        backdrop_blur: f32,
        children: NodeVec,
    },
    // A portal: its subtree is hoisted to the top layer at compose time (drawn last, above everything, and
    // escaping any ancestor clip/transform/layer). Used for overlays — dropdowns, modals, drawers, toasts.
    // Positioning is the caller's job (lay the content out where it should appear, e.g. an absolute-fill box).
    Overlay {
        children: NodeVec,
    },
    // A reactive boundary: the child segment maintains its own flattened commands via its own effect, so the parent's view() references it without re-running the child's view(). Composed lazily at collect time (see segment.rs). Enables O(changed component) updates instead of O(tree).
    Boundary {
        child: std::rc::Rc<crate::segment::Segment>,
    },
}

impl RenderNode {
    pub fn group(children: impl IntoIterator<Item = RenderNode>) -> Self {
        Self::Group {
            children: NodeVec::collect(children),
        }
    }

    pub fn rect(rect: Rect, style: RectStyle) -> Self {
        Self::Primitive(DrawCommand::Rect {
            rect,
            style: Arc::new(style),
        })
    }

    pub fn text(text: impl Into<Arc<str>>, rect: Rect, style: TextStyle) -> Self {
        Self::Primitive(DrawCommand::Text {
            text: text.into(),
            rect,
            style: Arc::new(style),
        })
    }

    pub fn rich_text(runs: Arc<[TextRun]>, rect: Rect, base: TextStyle) -> Self {
        Self::Primitive(DrawCommand::RichText {
            runs,
            rect,
            base: Arc::new(base),
        })
    }

    pub fn path(data: Arc<PathData>, style: PathStyle) -> Self {
        Self::Primitive(DrawCommand::Path {
            data,
            style: Arc::new(style),
        })
    }

    pub fn transform_with(
        matrix: [f32; 6],
        children: impl IntoIterator<Item = RenderNode>,
    ) -> Self {
        Self::Transform {
            matrix,
            children: NodeVec::collect(children),
        }
    }

    pub fn layer(
        opacity: f32,
        backdrop_blur: f32,
        children: impl IntoIterator<Item = RenderNode>,
    ) -> Self {
        Self::Layer {
            opacity,
            backdrop_blur,
            children: NodeVec::collect(children),
        }
    }

    /// A portal whose subtree is hoisted to the top layer at compose time (see [`RenderNode::Overlay`]).
    pub fn overlay(children: impl IntoIterator<Item = RenderNode>) -> Self {
        Self::Overlay {
            children: NodeVec::collect(children),
        }
    }
}
