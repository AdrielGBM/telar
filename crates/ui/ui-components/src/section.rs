use layout_core::{LayoutError, LayoutStyle};
use ui_core::{Container, LayoutItem, Slots, box_item};

use crate::heading::{HeadingProps, heading};
use crate::shared;
use crate::shared::props_default;

/// A titled column: a `heading` above its slot children in a small-gap `flex_column`. High-level sugar;
/// lives in `ui-components`, not the kernel.
pub struct SectionProps {
    pub title: Box<dyn Fn() -> String>,
}

props_default!(SectionProps { title: text });

pub fn section(props: SectionProps, mut slots: Slots) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut children: Vec<Box<dyn LayoutItem>> = vec![heading(HeadingProps { text: props.title })?];
    children.extend(slots.take_default());
    let col = Container::new(
        LayoutStyle::new().flex_column().gap(shared::spacing()),
        children,
    )?;
    Ok(box_item(col))
}
