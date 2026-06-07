use std::cell::RefCell;

use geometry_core::Rect;
use renderer_core::{BorderRadius, DrawCommand};

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
        // Move the inner vec out before Drop runs so we don't clear it prematurely.
        // We use ManuallyDrop to skip NodeVec's drop, then return the vec's iterator.
        let mut md = std::mem::ManuallyDrop::new(self);
        let v = std::mem::take(&mut md.0);
        // Return the vec to the pool after iteration completes is not straightforward here,
        // so we accept the allocation cost for into_iter (flatten_view path) in exchange for
        // correctness. The pool is still beneficial for the common short-lived collect+drop path.
        v.into_iter()
    }
}

pub enum RenderNode {
    Empty,
    Primitive(DrawCommand),
    Group(NodeVec),
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
}

impl RenderNode {
    pub fn group(children: impl IntoIterator<Item = RenderNode>) -> Self {
        Self::Group(NodeVec::collect(children))
    }
}
