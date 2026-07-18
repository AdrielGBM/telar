use layout_core::{LayoutError, LayoutStyle};
use ui_core::{Container, LayoutItem, Slots, Text, box_item};

use crate::heading::heading_style;

/// A titled column: a `heading` above its slot children in a small-gap `flex_column`. High-level sugar;
/// lives in `ui-components`, not the kernel.
pub struct SectionProps {
    pub title: Box<dyn Fn() -> String>,
}

impl Default for SectionProps {
    fn default() -> Self {
        Self {
            title: Box::new(String::new),
        }
    }
}

pub fn section(props: SectionProps, mut slots: Slots) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = props.title;
    let heading = Text::new(
        move || title(),
        LayoutStyle::new().height(20.0 * 1.4),
        heading_style,
    )?;
    let mut children: Vec<Box<dyn LayoutItem>> = vec![box_item(heading)];
    children.extend(slots.take_default());
    let col = Container::new(LayoutStyle::new().flex_column().gap(8.0), children)?;
    Ok(box_item(col))
}
