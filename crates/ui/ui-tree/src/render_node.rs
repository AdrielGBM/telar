//! [`RenderNode`]: what a `view()` returns, before it is flattened into draw commands.

use std::cell::RefCell;
use std::sync::Arc;

use geometry_core::Rect;
use renderer_core::{BorderRadius, DrawCommand, PathData, PathStyle, RectStyle, Span, TextStyle};

thread_local! {
    static NODE_VEC_POOL: RefCell<Vec<Vec<RenderNode>>> = const { RefCell::new(Vec::new()) };
}

/// A pooled `Vec<RenderNode>`, returned to the pool when it drops.
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
        // `ManuallyDrop` suppresses `NodeVec`'s `Drop`, which would return the vec to the pool.
        let mut md = std::mem::ManuallyDrop::new(self);
        let v = std::mem::take(&mut md.0);
        v.into_iter()
    }
}

/// What a `view()` returns: a draw command, a group of them, or a boundary the compositor treats specially.
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
    /// A box, for a backend whose output is a document: its identity, what it means, and what it holds.
    ///
    /// Emitted only while [`element_capture`](crate::element_capture) is on. A rasteriser flattens straight through it — the commands inside are already positioned — so the only cost where nobody reads it is not building it at all.
    Element {
        element: std::sync::Arc<renderer_core::Element>,
        children: NodeVec,
    },
    // A portal: its subtree is hoisted to the top layer at compose time, drawn last and escaping any ancestor clip, transform or layer. Positioning is the caller's job.
    Overlay {
        children: NodeVec,
    },
    // A reactive boundary: the child segment maintains its own flattened commands via its own effect.
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
            spans: None,
            rect,
            style: Arc::new(style),
        })
    }

    /// [`text`](Self::text) with byte ranges that style themselves differently from the paragraph.
    pub fn spanned_text(
        text: impl Into<Arc<str>>,
        spans: Arc<[Span]>,
        rect: Rect,
        style: TextStyle,
    ) -> Self {
        Self::Primitive(DrawCommand::Text {
            text: text.into(),
            spans: (!spans.is_empty()).then_some(spans),
            rect,
            style: Arc::new(style),
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

    /// A pure translation, which is what almost every `Transform` in a widget is: a leaf drawing its content in local coordinates and moving it to wherever the layout put it. Spelling it out as a matrix is six numbers where two are meant, and four of them have to be read to see it is not a scale or a rotation.
    pub fn translate(dx: f32, dy: f32, children: impl IntoIterator<Item = RenderNode>) -> Self {
        Self::Transform {
            matrix: [1.0, 0.0, 0.0, 1.0, dx, dy],
            children: NodeVec::collect(children),
        }
    }

    /// Cuts its subtree to `rect`, which the renderer maps through the active matrix — so a widget clipping itself passes its own local box and the clip composes with whatever moved it there.
    pub fn clip(
        rect: Rect,
        radius: BorderRadius,
        children: impl IntoIterator<Item = RenderNode>,
    ) -> Self {
        Self::Clip {
            rect,
            radius,
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

    /// Wraps `children` as one box a document backend can reconcile. See [`RenderNode::Element`].
    pub fn element(
        element: std::sync::Arc<renderer_core::Element>,
        children: impl IntoIterator<Item = RenderNode>,
    ) -> Self {
        Self::Element {
            element,
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
