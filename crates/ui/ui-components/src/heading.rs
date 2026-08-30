use layout_core::{LayoutError, LayoutStyle};
use reactive_core::Reactive;
use telar_macros::Props;
use ui_core::{LayoutItem, Text, box_item};

/// A section title: semibold, half again the body size around it, coloured from the theme's accent
/// (`primary`). High-level sugar over `text`; lives in `ui-components`, not the kernel.
#[derive(Props)]
pub struct HeadingProps {
    #[props(into, default)]
    pub text: Reactive<String>,
}

pub fn heading(props: HeadingProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = props.text;
    // No pinned height: a measured leaf sizes itself from the text it resolved to, so a title follows a
    // declared body size without an effect having to be owned somewhere to push a number at it.
    let t = Text::declaring(move || text.get(), LayoutStyle::new(), heading_style)?;
    Ok(box_item(t))
}

pub(crate) use crate::shared::heading_style;
