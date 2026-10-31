//! [`section`]: a titled block that groups the content nested inside it.

use telar::{Children, Container, LayoutError, LayoutItem, LayoutStyle, Props, Reactive, box_item};

use crate::heading::{HeadingProps, heading};
use crate::shared;

/// A titled column: a `heading` above its `children` in a small-gap `flex_column`. High-level sugar; lives in `telar-components`, not the kernel.
#[derive(Props)]
pub struct SectionProps {
    #[props(into, default)]
    pub title: Reactive<String>,
    /// The content under the title.
    #[props(default)]
    pub children: Children,
}

/// A titled block grouping the content nested inside it.
pub fn section(props: SectionProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let SectionProps { title, children } = props;
    let content = children.build()?;
    let mut column_items: Vec<Box<dyn LayoutItem>> =
        vec![heading(HeadingProps::props().text(title).build())?];
    column_items.extend(content);
    let column = || LayoutStyle::column().gap(shared::gap_md());
    let col = Container::new(column(), column_items)?.styled_by(column);
    Ok(box_item(col))
}
