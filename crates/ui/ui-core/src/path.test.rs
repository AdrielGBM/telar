use geometry_core::Point;
use layout_core::{AvailableSpace, NodeId, SizeDimension};
use renderer_core::{Color, DrawCommand, ShapeStyle, Stroke};
use ui_tree::ComponentList;

use super::*;
use crate::context::{compute_layout, new_container, reset_layout_runtime};
use crate::layout_item::LayoutItem;

fn icon() -> Arc<PathData> {
    Arc::new(PathData::polygon(&[
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
    ]))
}

fn filling() -> LayoutStyle {
    LayoutStyle::new()
        .width(SizeDimension::Percent(1.0))
        .height(SizeDimension::Percent(1.0))
}

fn mounted(path: Path) -> (NodeId, ComponentList) {
    let root = new_container(filling(), &[path.layout_node()]).unwrap();
    (root, ComponentList::new(path))
}

fn lay_out(root: NodeId, width: f32, height: f32) {
    compute_layout(
        root,
        AvailableSpace::Definite(width),
        AvailableSpace::Definite(height),
    )
    .unwrap();
}

fn drawn(tree: &ComponentList) -> (Arc<PathData>, Arc<PathStyle>) {
    tree.commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Path { data, style } => Some((data.clone(), style.clone())),
            _ => None,
        })
        .expect("the path reached the frame")
}

fn stroke_color(tree: &ComponentList) -> Color {
    let (_, style) = drawn(tree);
    match style.stroke.as_ref().expect("a stroke").paint {
        renderer_core::Paint::Solid(color) => color,
        ref other => panic!("expected a solid stroke, got {other:?}"),
    }
}

/// Artwork in units of its own is scaled into whatever box layout gives it, and again when that box changes.
#[test]
fn a_path_is_drawn_scaled_to_the_box_it_is_laid_out_in() {
    reset_layout_runtime();
    let view_box = Rect::new(0.0, 0.0, 10.0, 10.0);
    let path = Path::new(filling(), move |rect| {
        (
            Arc::new(icon().fitted(view_box, rect)),
            PathStyle::default(),
        )
    })
    .unwrap();
    let (root, tree) = mounted(path);

    lay_out(root, 40.0, 40.0);
    assert_eq!(
        drawn(&tree).0.bounds(),
        Some(Rect::new(0.0, 0.0, 40.0, 40.0))
    );

    lay_out(root, 20.0, 20.0);
    assert_eq!(
        drawn(&tree).0.bounds(),
        Some(Rect::new(0.0, 0.0, 20.0, 20.0)),
        "a smaller box draws the same artwork smaller"
    );
}

/// The paint is read on every paint, so a value it reads repaints the path rather than freezing at what it was when the path was built.
#[test]
fn a_path_repaints_when_what_its_paint_reads_changes() {
    reset_layout_runtime();
    let ink = reactive_core::signal(Color::RED);
    let path = Path::new(filling(), move |_| {
        (
            icon(),
            PathStyle::default().with_stroke(Stroke::new(ink.get(), 1.0)),
        )
    })
    .unwrap();
    let (root, tree) = mounted(path);
    lay_out(root, 10.0, 10.0);
    assert_eq!(stroke_color(&tree), Color::RED);

    ink.set(Color::BLUE);
    assert_eq!(stroke_color(&tree), Color::BLUE);
}

/// A drawn mark follows the ink the region around it declares, as the text beside it does, and follows it again when the region declares another.
#[test]
fn a_declaring_path_strokes_in_the_ink_around_it() {
    reset_layout_runtime();
    let path = Path::declaring(filling(), |_, text| {
        (
            icon(),
            PathStyle::default().with_stroke(Stroke::new(text.color, 1.0)),
        )
    })
    .unwrap();
    let (root, tree) = mounted(path);
    let first = Color::rgba(0.9, 0.2, 0.1, 1.0);
    crate::declare(root, renderer_core::Declared::default().with_color(first));
    lay_out(root, 10.0, 10.0);
    assert_eq!(stroke_color(&tree), first);

    let second = Color::rgba(0.1, 0.3, 0.8, 1.0);
    crate::declare(root, renderer_core::Declared::default().with_color(second));
    assert_eq!(stroke_color(&tree), second);
}

/// Laid out to nothing, a path draws nothing: its artwork is in its own coordinates and would otherwise land over whatever made room for it.
#[test]
fn a_collapsed_path_draws_nothing() {
    reset_layout_runtime();
    let path = Path::new(LayoutStyle::new().width(0.0).height(10.0), |_| {
        (icon(), PathStyle::default().with_fill(Color::RED))
    })
    .unwrap();
    let (root, tree) = mounted(path);
    lay_out(root, 10.0, 10.0);

    assert!(
        !tree
            .commands()
            .iter()
            .any(|command| matches!(command, DrawCommand::Path { .. }))
    );
}
