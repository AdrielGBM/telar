use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, TextStyle};
use ui_core::{LayoutItem, StyledContainer, Text, box_item};

use crate::shared;
use crate::shared::props_default;

fn pad_x() -> f32 {
    shared::spacing()
}
fn pad_y() -> f32 {
    shared::spacing() * 0.25
}
/// A badge's share of the text around it: a tag reads as an annotation, not as a sentence.
const TEXT_RATIO: f32 = 0.85;
fn radius() -> f32 {
    shared::radius() * 2.5
}

fn pill() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_horizontal(pad_x())
        .padding_vertical(pad_y())
}

/// A small solid pill tag: an accent-filled box with a short label in a contrasting on-accent colour.
/// Non-interactive (unlike `button`) — pure presentation sugar over `StyledContainer` + `Text`; lives in
/// `ui-components`, not the kernel, so an app can drop it or ship its own.
pub struct BadgeProps {
    pub label: Box<dyn Fn() -> String>,
    /// Fill colour. `Color::TRANSPARENT` (the default) means "unset" -> the theme's `primary()`. A
    /// closure (re-read every frame) so a theme token or `$signal` colour re-colours live, like `button`'s `fill`.
    pub color: Box<dyn Fn() -> Color>,
}

props_default!(BadgeProps {
    label: text,
    color: color,
});

pub fn badge(props: BadgeProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let BadgeProps { label, color } = props;

    let label_widget = Text::declaring(move || label(), LayoutStyle::new(), on_accent_style)?;

    let container = StyledContainer::new(
        pill(),
        move |_r| {
            RectStyle::default()
                .with_fill(shared::resolve(color.as_ref(), fill_default))
                .with_radius(BorderRadius::all(radius()))
        },
        vec![box_item(label_widget)],
    )?
    .styled_by(pill);
    Ok(box_item(container))
}

/// The default pill fill when `color` is unset: the theme's primary accent, matching `button`'s own default.
fn fill_default() -> Color {
    shared::accent()
}

/// The label's on-accent colour, re-read every frame so it tracks the active theme (mirrors `button`'s
/// no-variant label default: the theme's `on_primary()`, or white with no theme installed).
fn on_accent_style(inherited: TextStyle) -> TextStyle {
    shared::control_text(inherited, TEXT_RATIO).with_color(shared::on_accent())
}

#[cfg(test)]
mod tests {
    use layout_core::AvailableSpace;
    use renderer_core::DrawCommand;
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
        crate::test_support::fresh_layout_runtime();
        let badge = badge(BadgeProps {
            label: Box::new(|| "New".to_string()),
            ..Default::default()
        })
        .unwrap();
        let tree = laid_out(badge);
        assert!(find_text(&tree.commands(), "New"));
    }

    // An empty label still builds and lays out without panicking.
    #[test]
    fn empty_label_builds_without_panic() {
        crate::test_support::fresh_layout_runtime();
        let badge = badge(BadgeProps::default()).unwrap();
        let tree = laid_out(badge);
        let _ = tree.commands();
    }
}
