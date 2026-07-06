use layout_core::{LayoutError, LayoutStyle};
use ui_core::{Container, LayoutItem, Slots, Text, WidgetCtx, box_item};

use crate::heading::heading_style;

/// A titled column: a `heading` above its slot children in a small-gap `flex_column`. High-level sugar;
/// lives in `ui-components`, not the kernel.
#[derive(Default)]
pub struct SectionProps {
    pub title: &'static str,
}

pub fn section(
    ctx: &mut WidgetCtx,
    props: SectionProps,
    mut slots: Slots,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = props.title;
    let heading = Text::new(
        ctx,
        move || title.to_string(),
        LayoutStyle::new().height(20.0 * 1.4),
        heading_style,
    )?;
    let mut children: Vec<Box<dyn LayoutItem>> = vec![box_item(heading)];
    children.extend(slots.take_default());
    let col = Container::new(ctx, LayoutStyle::new().flex_column().gap(8.0), children)?;
    Ok(box_item(col))
}
