use super::*;
use crate::{
    ComponentList, DrawCommand, Event, Rect, RectStyle, ShapeStyle, SizeDimension, StyledContainer,
};

const SWATCH: Color = Color::rgba(0.2, 0.6, 0.3, 1.0);
const HEIGHT: f32 = 600.0;

fn filling_swatch(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(StyledContainer::new(
        LayoutStyle::new()
            .height(SizeDimension::Percent(1.0))
            .min_height(50.0),
        |_| RectStyle::default().with_fill(SWATCH),
        vec![],
    )?))
}

fn drawn(tree: &ComponentList) -> (Vec<Rect>, Vec<(String, Rect)>) {
    let (mut swatches, mut texts) = (Vec::new(), Vec::new());
    crate::for_each_with_matrix(&tree.commands(), |command, matrix| match command {
        DrawCommand::Rect { rect, style, .. } if style.fill == Some(SWATCH.into()) => {
            swatches.push(crate::transform_clip_rect(matrix, *rect))
        }
        DrawCommand::Text { text, rect, .. } => {
            texts.push((text.to_string(), crate::transform_clip_rect(matrix, *rect)))
        }
        _ => {}
    });
    (swatches, texts)
}

#[test]
fn the_page_keeps_each_preview_at_its_content_height_one_under_another() {
    let app = PreviewApp::new(vec![
        PreviewEntry::new("demo--box--a", "box", "A", filling_swatch),
        PreviewEntry::new("demo--box--b", "box", "B", filling_swatch),
    ]);
    let mut tree = ComponentList::new(app.page(None));
    tree.on_event(&Event::WindowResized {
        width: 800,
        height: HEIGHT as u32,
    });
    crate::relayout_if_dirty();

    let (swatches, texts) = drawn(&tree);
    let [first, second] = swatches[..] else {
        panic!("two previews are drawn: {swatches:?}");
    };
    assert_eq!(
        (first.height, second.height),
        (50.0, 50.0),
        "a preview that fills its parent is still its content height on the page"
    );
    let header = texts
        .iter()
        .find(|(text, _)| text.contains("B"))
        .map(|(_, rect)| *rect)
        .expect("the second header is drawn");
    assert_eq!(
        header.y,
        first.y + first.height + 16.0 + 16.0 + 16.0 + 8.0,
        "the next section starts right under the first preview's own section"
    );
    assert!(second.y > header.y && second.y + second.height < HEIGHT);
}
