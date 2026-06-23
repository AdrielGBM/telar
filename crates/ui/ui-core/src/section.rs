use layout_core::{LayoutError, LayoutStyle};
use renderer_core::{Color, TextStyle};
use theme_core::use_widget_theme;

use crate::container::Container;
use crate::context::WidgetCtx;
use crate::layout_item::{LayoutItem, box_item};
use crate::text::Text;

const HEADING_FONT_SIZE: f32 = 12.0;
const SECTION_GAP: f32 = 8.0;

/// A muted caption label styled from the active `WidgetTheme` (`widget_muted`),
/// theme-agnostic so it works without knowing the concrete theme type. Encodes
/// the recurring `text size:12 color:muted` section-title pattern.
pub struct Heading;

impl Heading {
    pub fn new(
        ctx: &mut WidgetCtx,
        content: impl Fn() -> String + 'static,
    ) -> Result<Text, LayoutError> {
        Text::single_line(ctx, content, || {
            let color = use_widget_theme()
                .map(|t| t.widget_muted())
                .unwrap_or(Color::rgba(0.5, 0.5, 0.6, 1.0));
            TextStyle::new(HEADING_FONT_SIZE, color)
        })
    }
}

/// A titled vertical section: a [`Heading`] above its content in a column with a
/// small gap. Replaces the repeated `col gap:8 { text size:12 color:muted; … }`.
pub struct Section;

impl Section {
    pub fn new(
        ctx: &mut WidgetCtx,
        title: impl Fn() -> String + 'static,
        content: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Container, LayoutError> {
        let heading = Heading::new(ctx, title)?;
        let mut children: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(content.len() + 1);
        children.push(box_item(heading));
        children.extend(content);
        Container::new(
            ctx,
            LayoutStyle::new().flex_column().gap(SECTION_GAP),
            children,
        )
    }
}
