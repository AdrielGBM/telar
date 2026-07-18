use layout_core::{LayoutError, LayoutStyle};
use renderer_core::{Color, TextStyle};
use theme_core::use_theme_tokens;
use ui_core::{LayoutItem, Text, box_item};

/// A section title: 20px, semibold, coloured from the theme's accent (`primary`). High-level
/// sugar over `text`; lives in `ui-components`, not the kernel.
pub struct HeadingProps {
    pub text: Box<dyn Fn() -> String>,
}

impl Default for HeadingProps {
    fn default() -> Self {
        Self {
            text: Box::new(String::new),
        }
    }
}

pub fn heading(props: HeadingProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = props.text;
    let t = Text::new(
        move || text(),
        LayoutStyle::new().height(20.0 * 1.4),
        heading_style,
    )?;
    Ok(box_item(t))
}

/// The shared title text style, re-read every frame so it tracks the active theme. Reused by `section`.
pub(crate) fn heading_style() -> TextStyle {
    let color = use_theme_tokens()
        .map(|t| t.primary())
        .unwrap_or(Color::rgba(0.1, 0.1, 0.12, 1.0));
    TextStyle::new(20.0, color).with_weight(600)
}
