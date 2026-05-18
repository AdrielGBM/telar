use renderer_core::DrawCommand;

pub enum View {
    Empty,
    Primitive(DrawCommand),
    Group(Vec<View>),
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
