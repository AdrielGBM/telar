//! [`badge`]: a small status label, filled from a semantic colour.

use telar::{
    AlignItems, BorderRadius, Children, Color, JustifyContent, LayoutError, LayoutItem,
    LayoutStyle, Props, Reactive, Rect, RectStyle, ShapeStyle, StyledContainer, SurfaceAmendment,
    Text, amend_surface, box_item, children,
};

use crate::shared;

/// A badge's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the badge paints and lays out, so a theme that moves the role still moves every badge it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BadgeTokens {
    /// Corner radius. `None` is a pill: fully rounded at whatever height the badge is laid out.
    pub radius: Option<f32>,
    /// Space between the edge and the label at the regular control size, scaled with the control size in force. `None` is [`metrics::gap_md`](crate::metrics::gap_md).
    pub pad_x: Option<f32>,
}

impl BadgeTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    /// The corner for a badge laid out at `rect`.
    pub fn radius(&self, rect: Rect) -> f32 {
        self.radius
            .unwrap_or_else(|| shared::pill_radius(rect.width.min(rect.height)))
    }

    pub fn pad_x(&self) -> f32 {
        self.pad_x
            .map_or_else(shared::gap_md, |pad| pad * telar::control_scale())
    }
}

fn pill_box() -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_horizontal(BadgeTokens::current().pad_x())
        .padding_vertical(shared::gap_xs())
}

/// A small solid pill tag: an accent-filled box with a short label, at a caption's share of the text around it, in whichever ink reads on that fill. Non-interactive (unlike `button`) — pure presentation sugar over `StyledContainer` + `Text`; lives in `telar-components`, not the kernel, so an app can drop it or ship its own.
#[derive(Props)]
pub struct BadgeProps {
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Fill colour. `Color::TRANSPARENT` (the default) means "unset" -> the theme's `primary()`. A closure (re-read every frame) so a theme token or `$signal` colour re-colours live, like `button`'s `fill`.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Amends the paint of the pill — the badge's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`BadgeTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A small status label, filled from a semantic colour.
pub fn badge(props: BadgeProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let BadgeProps {
        label,
        color,
        style,
    } = props;

    let ink_fill = color.clone();
    let label_widget = Text::declaring(
        move || label.get(),
        LayoutStyle::new(),
        move |inherited| {
            shared::control_text(inherited, shared::CAPTION_RATIO)
                .with_color(shared::ink_on(shared::resolve(&ink_fill, shared::accent)))
        },
    )?;

    let container = StyledContainer::new(
        pill_box(),
        move |r| {
            let rect = RectStyle::default()
                .with_fill(shared::resolve(&color, shared::accent))
                .with_radius(BorderRadius::all(BadgeTokens::current().radius(r)));
            amend_surface(rect, &style)
        },
        children![label_widget],
    )?
    .styled_by(pill_box);
    Ok(box_item(container))
}

#[cfg(test)]
#[path = "badge_test.rs"]
mod tests;
