use layout_core::{LayoutError, LayoutStyle};
use reactive_core::Reactive;
use telar_macros::Props;
use ui_core::{Container, LayoutItem, Slots, box_item};

use crate::heading::{HeadingProps, heading};
use crate::shared;

/// A titled column: a `heading` above its slot children in a small-gap `flex_column`. High-level sugar;
/// lives in `ui-components`, not the kernel.
#[derive(Props)]
pub struct SectionProps {
    #[props(into, default)]
    pub title: Reactive<String>,
}

pub fn section(props: SectionProps, mut slots: Slots) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut children: Vec<Box<dyn LayoutItem>> =
        vec![heading(HeadingProps::props().text(props.title).build())?];
    children.extend(slots.take_default());
    let col = Container::new(
        LayoutStyle::new().flex_column().gap(shared::spacing()),
        children,
    )?;
    Ok(box_item(col))
}
