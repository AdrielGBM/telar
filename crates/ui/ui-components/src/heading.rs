use crate::shared::props_default;
use layout_core::{LayoutError, LayoutStyle};
use ui_core::{LayoutItem, Text, box_item};

/// A section title: 20px, semibold, coloured from the theme's accent (`primary`). High-level
/// sugar over `text`; lives in `ui-components`, not the kernel.
pub struct HeadingProps {
    pub text: Box<dyn Fn() -> String>,
}

props_default!(HeadingProps { text: text });

pub fn heading(props: HeadingProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = props.text;
    let t = Text::new(
        move || text(),
        LayoutStyle::new().height(20.0 * 1.4),
        heading_style,
    )?;
    Ok(box_item(t))
}

pub(crate) use crate::shared::heading_style;
