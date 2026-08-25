use crate::shared::props_default;
use layout_core::{LayoutError, LayoutStyle};
use ui_core::{LayoutItem, Text, box_item};

/// A section title: semibold, half again the body size around it, coloured from the theme's accent
/// (`primary`). High-level sugar over `text`; lives in `ui-components`, not the kernel.
pub struct HeadingProps {
    pub text: Box<dyn Fn() -> String>,
}

props_default!(HeadingProps { text: text });

pub fn heading(props: HeadingProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = props.text;
    // No pinned height: a measured leaf sizes itself from the text it resolved to, so a title follows a
    // declared body size without an effect having to be owned somewhere to push a number at it.
    let t = Text::declaring(move || text(), LayoutStyle::new(), heading_style)?;
    Ok(box_item(t))
}

pub(crate) use crate::shared::heading_style;
