use renderer_core::{DrawCommand, Rect};

pub enum View {
    Empty,
    Primitive(DrawCommand),
    Group(Vec<View>),
    Translate {
        tx: f32,
        ty: f32,
        children: Vec<View>,
    },
    Clip {
        rect: Rect,
        children: Vec<View>,
    },
    Layer {
        opacity: f32,
        children: Vec<View>,
    },
}

impl View {
    pub fn group(children: impl IntoIterator<Item = View>) -> Self {
        Self::Group(children.into_iter().collect())
    }
}
