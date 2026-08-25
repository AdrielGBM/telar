//! A path is a child like any other.
//!
//! It used to be a `Component` with no layout node, so the only way into a tree was a `Canvas` wrapped
//! around it — a widget the caller had to know to write, and one the transpiler emitted around every `path`
//! in markup. B8's whole complaint: a primitive that cannot be a child of a `col`.

use telar::{
    AvailableSpace, Color, Component, DrawCommand, LayoutItem, LayoutStyle, Path, PathData,
    PathStyle, Point, RenderNode, ShapeStyle, box_item, compute_layout, new_container,
    reset_layout_runtime,
};

struct Root(Box<dyn LayoutItem>);
impl Component for Root {
    fn view(&self) -> RenderNode {
        self.0.view()
    }
    fn on_event(&mut self, _: &platform_core::Event) -> ui_core::EventResult {
        ui_core::EventResult::Ignored
    }
    fn debug_name(&self) -> &'static str {
        "PathRoot"
    }
}

fn triangle() -> std::sync::Arc<PathData> {
    std::sync::Arc::new(PathData::polygon(&[
        Point::new(0.0, 0.0),
        Point::new(60.0, 0.0),
        Point::new(30.0, 40.0),
    ]))
}

/// Placed by its siblings, in a row, at a position nothing about the path itself decided.
#[test]
fn a_path_is_placed_by_the_row_it_sits_in() {
    reset_layout_runtime();
    // A rect is the neighbour; the path is what has to follow it.
    let lead = telar::Rectangle::new(LayoutStyle::new().width(100.0).height(40.0), || {
        telar::RectStyle::default().with_fill(Color::BLACK)
    })
    .unwrap();
    let path = Path::static_data(
        LayoutStyle::new().width(60.0).height(40.0),
        triangle(),
        || PathStyle::default().with_fill(Color::rgba(1.0, 0.0, 0.0, 1.0)),
    )
    .unwrap();
    let row = new_container(
        LayoutStyle::new().flex_row(),
        &[lead.layout_node(), path.layout_node()],
    )
    .unwrap();
    compute_layout(
        row,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let rect = telar::track_layout(path.layout_node()).unwrap().get();
    assert_eq!(rect.x, 100.0, "the path starts where its neighbour ends");
    assert_eq!(rect.width, 60.0);
}

/// And what it draws lands at that position: the geometry is in the path's own coordinates, and the box it
/// was given is what moves it.
#[test]
fn a_paths_geometry_is_drawn_where_layout_put_it() {
    reset_layout_runtime();
    let lead = telar::Rectangle::new(LayoutStyle::new().width(100.0).height(40.0), || {
        telar::RectStyle::default().with_fill(Color::BLACK)
    })
    .unwrap();
    let path = Path::static_data(
        LayoutStyle::new().width(60.0).height(40.0),
        triangle(),
        || PathStyle::default().with_fill(Color::rgba(1.0, 0.0, 0.0, 1.0)),
    )
    .unwrap();
    let row = telar::Container::new(
        LayoutStyle::new().flex_row(),
        vec![box_item(lead), box_item(path)],
    )
    .unwrap();
    let root = row.layout_node();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let tree = ui_core::ComponentList::new(Root(box_item(row)));
    let commands = tree.commands();
    // The matrix in force where the geometry lands, not the first one in the frame: the rect beside it
    // wears one too, at the origin.
    let mut translation = None;
    for command in commands.iter() {
        match command {
            DrawCommand::PushMatrix { matrix } => translation = Some((matrix[4], matrix[5])),
            DrawCommand::Path { .. } => break,
            _ => {}
        }
    }
    assert_eq!(
        translation,
        Some((100.0, 0.0)),
        "translated by the row, not drawn at its own origin"
    );
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, DrawCommand::Path { .. })),
        "and the geometry reached the frame"
    );
}
