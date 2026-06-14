use std::cell::RefCell;
use std::sync::Arc;

use geometry_core::Rect;
use renderer_core::{
    BorderRadius, DrawCommand, FRAME_STYLE_POOL, PathData, PathStyle, RectStyle, TextStyle,
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
    Group {
        node_key: u64,
        children: NodeVec,
    },
    Transform {
        node_key: u64,
        matrix: [f32; 6],
        children: NodeVec,
    },
    Clip {
        node_key: u64,
        rect: Rect,
        radius: BorderRadius,
        children: NodeVec,
    },
    Layer {
        node_key: u64,
        opacity: f32,
        backdrop_blur: f32,
        children: NodeVec,
    },
}

impl RenderNode {
    pub fn group(children: impl IntoIterator<Item = RenderNode>) -> Self {
        Self::Group {
            node_key: 0,
            children: NodeVec::collect(children),
        }
    }

    pub fn group_keyed(key: u64, children: impl IntoIterator<Item = RenderNode>) -> Self {
        Self::Group {
            node_key: key,
            children: NodeVec::collect(children),
        }
    }

    pub fn rect(rect: Rect, style: RectStyle) -> Self {
        let handle = FRAME_STYLE_POOL.lock().unwrap().intern_rect(style);
        Self::Primitive(DrawCommand::Rect {
            rect,
            style: handle,
        })
    }

    pub fn text(text: impl Into<Arc<str>>, rect: Rect, style: TextStyle) -> Self {
        let handle = FRAME_STYLE_POOL.lock().unwrap().intern_text(style);
        Self::Primitive(DrawCommand::Text {
            text: text.into(),
            rect,
            style: handle,
        })
    }

    pub fn path(data: Arc<PathData>, style: PathStyle) -> Self {
        let handle = FRAME_STYLE_POOL.lock().unwrap().intern_path(style);
        Self::Primitive(DrawCommand::Path {
            data,
            style: handle,
        })
    }

    pub fn layer(
        opacity: f32,
        backdrop_blur: f32,
        children: impl IntoIterator<Item = RenderNode>,
    ) -> Self {
        Self::Layer {
            node_key: 0,
            opacity,
            backdrop_blur,
            children: NodeVec::collect(children),
        }
    }

    pub fn layer_keyed(
        key: u64,
        opacity: f32,
        backdrop_blur: f32,
        children: impl IntoIterator<Item = RenderNode>,
    ) -> Self {
        Self::Layer {
            node_key: key,
            opacity,
            backdrop_blur,
            children: NodeVec::collect(children),
        }
    }

    pub fn transform_with(
        matrix: [f32; 6],
        children: impl IntoIterator<Item = RenderNode>,
    ) -> Self {
        Self::Transform {
            node_key: 0,
            matrix,
            children: NodeVec::collect(children),
        }
    }

    pub fn transform_keyed(
        key: u64,
        matrix: [f32; 6],
        children: impl IntoIterator<Item = RenderNode>,
    ) -> Self {
        Self::Transform {
            node_key: key,
            matrix,
            children: NodeVec::collect(children),
        }
    }
}
