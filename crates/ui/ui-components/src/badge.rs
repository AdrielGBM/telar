use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, TextStyle};
use theme_core::use_theme_tokens;
use ui_core::{LayoutItem, StyledContainer, Text, box_item};

use crate::shared;

const PAD_X: f32 = 8.0;
const PAD_Y: f32 = 2.0;
const FONT_SIZE: f32 = 12.0;
const RADIUS: f32 = 10.0;

/// A small solid pill tag: an accent-filled box with a short label in a contrasting on-accent colour.
/// Non-interactive (unlike `button`) — pure presentation sugar over `StyledContainer` + `Text`; lives in
/// `ui-components`, not the kernel, so an app can drop it or ship its own.
pub struct BadgeProps {
    pub label: &'static str,
    /// Fill colour. `Color::TRANSPARENT` (the default) means "unset" -> the theme's `primary()`. A
    /// closure (re-read every frame) so a theme token or `$signal` colour re-colours live, like `button`'s `fill`.
    pub color: Box<dyn Fn() -> Color>,
}

impl Default for BadgeProps {
    fn default() -> Self {
        Self {
            label: "",
            color: Box::new(|| Color::TRANSPARENT),
        }
    }
}

pub fn badge(props: BadgeProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let BadgeProps { label, color } = props;

    // `auto` (measured leaf) so the label has intrinsic width inside the pill; a stretched `Text::new` would
    // collapse to 0-wide, like `button`'s label. An empty label still measures fine (0-wide), so the pill
    // just shrinks to its padding rather than panicking.
    let label_widget = Text::auto(
        move || label.to_string(),
        LayoutStyle::new(),
        on_accent_style,
    )?;

    let container = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_horizontal(PAD_X)
            .padding_vertical(PAD_Y),
        move |_r| {
            RectStyle::default()
                .with_fill(shared::resolve(color.as_ref(), fill_default))
                .with_radius(BorderRadius::all(RADIUS))
        },
        vec![box_item(label_widget)],
    )?;
    Ok(box_item(container))
}

/// The default pill fill when `color` is unset: the theme's primary accent, matching `button`'s own default.
fn fill_default() -> Color {
    use_theme_tokens()
        .map(|t| t.primary())
        .unwrap_or(shared::DEFAULT_ACCENT)
}

/// The label's on-accent colour, re-read every frame so it tracks the active theme (mirrors `button`'s
/// no-variant label default: the theme's `on_primary()`, or white with no theme installed).
fn on_accent_style() -> TextStyle {
    let color = use_theme_tokens()
        .map(|t| t.on_primary())
        .unwrap_or(Color::WHITE);
    TextStyle::new(FONT_SIZE, color)
}

#[cfg(test)]
mod tests {
    use layout_core::AvailableSpace;
    use renderer_core::DrawCommand;
    use ui_core::reset_layout_runtime;
    use ui_core::{ComponentList, compute_layout, new_container};

    use super::*;

    fn find_text(cmds: &[DrawCommand], needle: &str) -> bool {
        cmds.iter()
            .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.as_ref() == needle))
    }

    fn laid_out(item: Box<dyn LayoutItem>) -> ComponentList {
        let root = new_container(
            LayoutStyle::new().flex_row().width(200.0).height(60.0),
            &[item.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(60.0),
        )
        .unwrap();
        ComponentList::new(item)
    }

    // A labelled badge draws its label text.
    #[test]
    fn renders_label() {
        reset_layout_runtime();
        let badge = badge(BadgeProps {
            label: "New",
            ..Default::default()
        })
        .unwrap();
        let tree = laid_out(badge);
        assert!(find_text(&tree.commands(), "New"));
    }

    // An empty label still builds and lays out without panicking.
    #[test]
    fn empty_label_builds_without_panic() {
        reset_layout_runtime();
        let badge = badge(BadgeProps::default()).unwrap();
        let tree = laid_out(badge);
        let _ = tree.commands();
    }
}
