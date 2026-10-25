use super::*;
use crate::{
    AvailableSpace, Color, ComponentList, Container, DrawCommand, Rect, RectStyle, ShapeStyle,
    SizeDimension, StyledContainer, compute_layout, reset_layout_runtime,
};

const SWATCH: Color = Color::rgba(0.2, 0.6, 0.3, 1.0);

fn filling_swatch(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(StyledContainer::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0))
            .min_height(50.0),
        |_| RectStyle::default().with_fill(SWATCH),
        vec![],
    )?))
}

fn unreachable_fallback(failure: BuildFailure) -> Result<Box<dyn LayoutItem>, LayoutError> {
    panic!("the preview failed: {failure}")
}

fn swatch_rect(tree: &ComponentList) -> Rect {
    tree.commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Rect { rect, style, .. } if style.fill == Some(SWATCH.into()) => {
                Some(*rect)
            }
            _ => None,
        })
        .expect("the swatch is drawn")
}

fn laid_out_in(style: LayoutStyle) -> Rect {
    reset_layout_runtime();
    let mount = remounting(PreviewCtx::default(), filling_swatch, unreachable_fallback).unwrap();
    let parent = Container::new(style, vec![mount]).unwrap();
    compute_layout(
        parent.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    swatch_rect(&ComponentList::new(parent))
}

#[test]
fn a_mounted_preview_fills_the_box_a_column_gives_it() {
    let rect = laid_out_in(LayoutStyle::new().flex_column().width(300.0).height(200.0));
    assert_eq!(rect, Rect::new(0.0, 0.0, 300.0, 200.0));
}

#[test]
fn a_mounted_preview_fills_the_box_a_row_gives_it() {
    let rect = laid_out_in(LayoutStyle::new().flex_row().width(300.0).height(200.0));
    assert_eq!(rect, Rect::new(0.0, 0.0, 300.0, 200.0));
}

fn segment(name: &'static str, depth: usize) -> SegmentNodeInfo {
    SegmentNodeInfo {
        id: 0,
        name: name.into(),
        depth,
        rect: crate::Rect::default(),
        padding: crate::Insets::default(),
        margin: crate::Insets::default(),
        border: crate::Insets::default(),
        gap: crate::Size::ZERO,
    }
}

#[test]
fn a_preview_s_own_nodes_are_those_under_the_boundary_it_is_mounted_in() {
    let walk = [
        segment("SurfaceRoot", 0),
        segment("Container", 1),
        segment("ReactiveList", 2),
        segment(ErrorBoundary::SEGMENT_NAME, 3),
        segment("Card", 4),
        segment("Text", 5),
        segment(ErrorBoundary::SEGMENT_NAME, 5),
        segment("Text", 6),
        segment("Overlay", 1),
    ];
    let nodes = preview_nodes(&walk);
    let own: Vec<(&str, usize)> = nodes.iter().map(|node| (&*node.name, node.depth)).collect();
    assert_eq!(
        own,
        [
            ("Card", 0),
            ("Text", 1),
            (ErrorBoundary::SEGMENT_NAME, 1),
            ("Text", 2)
        ],
        "a boundary of the preview's own is kept, and what the host has beside the preview is left out"
    );
    let bare = [segment("SurfaceRoot", 0), segment("Text", 1)];
    assert_eq!(
        preview_nodes(&bare).len(),
        2,
        "a walk with no boundary is kept whole"
    );
}
