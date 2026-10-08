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
