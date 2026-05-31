use geometry_core::Rect;
use renderer_core::{BorderRadius, DrawCommand};

pub enum RenderNode {
    Empty,
    Primitive(DrawCommand),
    Group(Vec<RenderNode>),
    Transform {
        matrix: [f32; 6],
        children: Vec<RenderNode>,
    },
    Clip {
        rect: Rect,
        radius: BorderRadius,
        children: Vec<RenderNode>,
    },
    Layer {
        opacity: f32,
        children: Vec<RenderNode>,
    },
}

impl RenderNode {
    pub fn group(children: impl IntoIterator<Item = RenderNode>) -> Self {
        Self::Group(children.into_iter().collect())
    }
}
