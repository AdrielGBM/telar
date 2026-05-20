use std::cell::RefCell;
use std::rc::Rc;

use renderer_core::{DrawCommand, Rect};

pub struct SubtreeHandle {
    pub(crate) commands: Rc<RefCell<Vec<DrawCommand>>>,
}

impl SubtreeHandle {
    pub(crate) fn new(commands: Rc<RefCell<Vec<DrawCommand>>>) -> Self {
        Self { commands }
    }
}

pub enum View {
    Empty,
    Primitive(DrawCommand),
    Subtree(SubtreeHandle),
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
}

impl View {
    pub fn empty() -> Self {
        Self::Empty
    }

    pub fn group(children: impl IntoIterator<Item = View>) -> Self {
        Self::Group(children.into_iter().collect())
    }
}

pub trait IntoView {
    fn into_view(self) -> View;
}

impl IntoView for View {
    fn into_view(self) -> View {
        self
    }
}

impl IntoView for DrawCommand {
    fn into_view(self) -> View {
        View::Primitive(self)
    }
}

impl IntoView for Vec<View> {
    fn into_view(self) -> View {
        View::Group(self)
    }
}

impl IntoView for () {
    fn into_view(self) -> View {
        View::Empty
    }
}
