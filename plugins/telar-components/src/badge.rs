//! [`badge`]: a small status label, filled from a semantic colour.

use telar::{
    AlignItems, BorderRadius, Children, Color, JustifyContent, LayoutError, LayoutItem,
    LayoutStyle, Props, Reactive, RectStyle, ShapeStyle, StyledContainer, Text, box_item,
};

use crate::shared;

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

/// A small solid pill tag: an accent-filled box with a short label in whichever ink reads on that fill. Non-interactive (unlike `button`) — pure presentation sugar over `StyledContainer` + `Text`; lives in `telar-components`, not the kernel, so an app can drop it or ship its own.
#[derive(Props)]
pub struct BadgeProps {
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Fill colour. `Color::TRANSPARENT` (the default) means "unset" -> the theme's `primary()`. A closure (re-read every frame) so a theme token or `$signal` colour re-colours live, like `button`'s `fill`.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
}

/// A small status label, filled from a semantic colour.
pub fn badge(props: BadgeProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let BadgeProps { label, color } = props;

    let ink_fill = color.clone();
    let label_widget = Text::declaring(
        move || label.get(),
        LayoutStyle::new(),
        move |inherited| {
            shared::control_text(inherited, TEXT_RATIO)
                .with_color(shared::ink_on(shared::resolve(&ink_fill, fill_default)))
        },
    )?;

    let container = StyledContainer::new(
        pill(),
        move |_r| {
            RectStyle::default()
                .with_fill(shared::resolve(&color, fill_default))
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

#[cfg(test)]
#[path = "badge_test.rs"]
mod tests;
