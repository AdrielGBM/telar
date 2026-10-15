//! [`chip`]: a compact pill, optionally with a status dot and a close affordance.

use std::rc::Rc;

use telar::{
    Accessible, AlignItems, Border, BorderRadius, Children, Color, Container, LayoutError,
    LayoutItem, LayoutStyle, Props, Reactive, Rect, RectStyle, ShapeStyle, StyledContainer,
    SurfaceAmendment, Text, amend_surface, box_item, children, focus::Role,
};

use crate::shared;
use crate::strings;

/// A chip's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the chip paints and lays out, so a theme that moves the role still moves every chip it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChipTokens {
    /// Corner radius. `None` is a pill: fully rounded at whatever height the chip is laid out.
    pub radius: Option<f32>,
    /// Space between the edge and the content at the regular control size, scaled with the control size in force. `None` is [`metrics::gap_md`](crate::metrics::gap_md).
    pub pad_x: Option<f32>,
    /// Width of the frame. `None` is the theme's `border_width`.
    pub border_width: Option<f32>,
}

impl ChipTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    /// The corner for a chip laid out at `rect`.
    pub fn radius(&self, rect: Rect) -> f32 {
        self.radius
            .unwrap_or_else(|| shared::pill_radius(rect.width.min(rect.height)))
    }

    pub fn pad_x(&self) -> f32 {
        self.pad_x
            .map_or_else(shared::gap_md, |pad| pad * telar::control_scale())
    }

    pub fn border_width(&self) -> f32 {
        self.border_width.unwrap_or_else(shared::border_width)
    }
}

fn dot_box() -> LayoutStyle {
    LayoutStyle::new()
        .width(shared::gap_md())
        .height(shared::gap_md())
}
fn inner_row() -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .gap(shared::gap_sm())
}
fn pill_box() -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .padding_horizontal(ChipTokens::current().pad_x())
        .padding_vertical(shared::gap_xs())
}

/// A small outlined tag, quieter than `badge`'s solid fill: a bordered surface pill with normal ink text, an optional small accent dot when `color` is set, and an optional close affordance that fires `on_close`. Non-interactive unless `on_close` is set. High-level sugar over `StyledContainer`/`Container` + `Text`; lives in `telar-components`, not the kernel, so an app can drop it or ship its own.
#[derive(Props)]
pub struct ChipProps {
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Small leading accent dot colour. `Color::TRANSPARENT` (the default) means "unset": no dot is shown at all (not just an invisible one) — see `chip`'s doc. A closure (re-read every frame) so a theme token or `$signal` colour re-colours the dot live, like `button`'s `fill`.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// When `Some`, a small close-mark press target renders on the right and calls it on tap. `None` (the default) omits it entirely, leaving a non-interactive chip.
    #[props(some, default)]
    pub on_close: Option<Rc<dyn Fn()>>,
    /// Amends the paint of the pill — the chip's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`ChipTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A compact pill, optionally with a status dot and a close affordance.
pub fn chip(props: ChipProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ChipProps {
        label,
        color,
        on_close,
        style,
    } = props;
    let mut children: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(3);

    // Presence is decided at build time — an unset colour means no dot, not an invisible one — while the shade is re-read every frame so a live theme colour tracks.
    if color.get() != Color::TRANSPARENT {
        let dot_color = color.clone();
        let dot = StyledContainer::new(dot_box(), move |r| dot_style(&dot_color, r), vec![])?
            .styled_by(dot_box);
        children.push(box_item(dot));
    }

    let name = label.clone();
    let label_widget = Text::declaring(
        move || label.get(),
        LayoutStyle::new(),
        |t| shared::control_text(t, shared::CAPTION_RATIO),
    )?;
    children.push(box_item(label_widget));

    if let Some(cb) = on_close {
        let close_mark = shared::mark(|| shared::Mark::Close, |text| text.color)?;
        let close = StyledContainer::new(
            LayoutStyle::row(),
            |_r| RectStyle::default(),
            vec![close_mark],
        )?
        .control(Role::Button)
        .a11y_label(move || strings::text_with(strings::REMOVE, &[("label", &name.get())]))
        .on_press(move || cb());
        children.push(box_item(close));
    }

    let row = Container::new(inner_row(), children)?.styled_by(inner_row);

    let pill = StyledContainer::new(
        pill_box(),
        move |r| {
            let tokens = ChipTokens::current();
            let rect = RectStyle::default()
                .with_fill(shared::surface_alt())
                .with_border(Border::uniform(shared::border(), tokens.border_width()))
                .with_radius(BorderRadius::all(tokens.radius(r)));
            amend_surface(rect, &style)
        },
        children![row],
    )?
    .styled_by(pill_box);
    Ok(box_item(pill))
}

fn dot_style(color: &Reactive<Color>, rect: Rect) -> RectStyle {
    RectStyle::default()
        .with_fill(color.get())
        .with_radius(shared::pill(rect))
}

#[cfg(test)]
#[path = "chip_test.rs"]
mod tests;
