//! [`button`]: the pressable control, in its filled, outline and ghost variants.

use std::rc::Rc;

use telar_macros::Props;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::{Reactive, signal};
use renderer_core::{Border, BorderRadius, Color, RectStyle, ShapeStyle};
use ui_core::focus::Role;
use ui_core::{Children, LayoutItem, StyledContainer, Text, box_item};

use crate::shared;

/// Padding a button reserves around its label, derived from the theme's spacing unit rather than fixed so one theme number moves it. `Text::new` measures the label at its full line box (taller than a bare `font_size * line_height`), which is why the vertical share is the lighter of the two.
fn pad_x() -> f32 {
    shared::spacing() * 1.75
}
fn pad_y() -> f32 {
    shared::spacing() * 0.75
}

/// A row so the label's measured width sets the box's main-axis size (a column would collapse the cross axis: `Text::new` sets `align_self_stretch`, which fights content-sizing and renders 0-wide).
fn shell() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_horizontal(pad_x())
        .padding_vertical(pad_y())
}

/// A labelled, pressable button. This is the high-level convenience over the primitives (`box` + `on_press` + `hover` + a centred `text`); it lives in `ui-components`, not the kernel, so an app can drop it or ship its own. `fill`/`outline` are reactive colour closures (re-read every frame) so a button styled from a theme token re-colours when the theme switches.
#[derive(Props)]
pub struct ButtonProps {
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Filled variant colour. `Color::TRANSPARENT` (the default) means "unset" — the button keeps its theme-driven default fill. A closure so a theme token re-reads on every render.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub fill: Reactive<Color>,
    /// Outlined variant colour; `Color::TRANSPARENT` means unset. Takes precedence only when `fill` is unset.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub outline: Reactive<Color>,
    #[props(default = false)]
    pub ghost: bool,
    #[props(default = Rc::new(|| {}))]
    pub on_press: Rc<dyn Fn()>,
}

/// The pressable control, in its filled, outline and ghost variants.
pub fn button(props: ButtonProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ButtonProps {
        label,
        fill,
        outline,
        ghost,
        on_press,
    } = props;
    // The colour closures feed three independent style closures, so they are shared via `Rc`. The container tracks its own hover for the rect swap, but the label's colour lives on a separate leaf, so the hover is mirrored into this signal for the label style to read.
    let hovered = signal(false);

    let (label_fill, label_outline, label_hover) = (fill.clone(), outline.clone(), hovered);
    let label_widget = Text::declaring(
        move || label.get(),
        LayoutStyle::new(),
        move |t| {
            let text = shared::control_text(t, 1.0);
            match label_color(&label_fill, &label_outline, ghost, label_hover.get()) {
                Some(color) => text.with_color(color),
                None => text,
            }
        },
    )?;

    let (base_fill, base_outline) = (fill.clone(), outline.clone());
    let (hover_fill, hover_outline) = (fill.clone(), outline.clone());
    let container = StyledContainer::new(
        shell(),
        move |_r| variant_rect(&base_fill, &base_outline, ghost, false),
        vec![box_item(label_widget)],
    )?
    .styled_by(shell)
    .hover_style(move |_r| variant_rect(&hover_fill, &hover_outline, ghost, true))
    .on_hover(move |h| hovered.set(h))
    .control(Role::Button)
    .on_press(move || on_press());
    Ok(box_item(container))
}

/// Resolves the box paint for the current frame and hover state from the variant inputs, re-reading the reactive `fill`/`outline` closures so a theme switch re-colours the button. Mirrors the old `ButtonStyle`: ghost is transparent, outline strokes then fills on hover, filled keeps its fill, and the no-variant default is the theme's primary (darkened on hover).
fn variant_rect(
    fill: &Reactive<Color>,
    outline: &Reactive<Color>,
    ghost: bool,
    hovered: bool,
) -> RectStyle {
    let radius = BorderRadius::all(shared::radius());
    if ghost {
        return RectStyle::default().with_radius(radius);
    }
    let outline_c = outline.get();
    if outline_c != Color::TRANSPARENT {
        return if hovered {
            RectStyle::default()
                .with_fill(outline_c)
                .with_radius(radius)
        } else {
            RectStyle::default()
                .with_border(Border::uniform(outline_c, 1.5))
                .with_radius(radius)
        };
    }
    let fill_c = fill.get();
    if fill_c != Color::TRANSPARENT {
        return RectStyle::default().with_fill(fill_c).with_radius(radius);
    }
    let primary = shared::accent();
    let base = if hovered {
        primary.darken(0.15)
    } else {
        primary
    };
    RectStyle::default().with_fill(base).with_radius(radius)
}

/// The label colour for the current frame and hover state: outline is its own colour (white on hover), filled contrasts with whatever it is filled with, and the no-variant default is the theme's on-primary.
///
/// `None` is the ghost variant, and means "whatever the page says". A ghost button paints no surface of its own, so its label sits on the page beside ordinary text — naming the theme's `ink` here made it the one word in a region that had declared its colour that came out in the theme's.
fn label_color(
    fill: &Reactive<Color>,
    outline: &Reactive<Color>,
    ghost: bool,
    hovered: bool,
) -> Option<Color> {
    if ghost {
        return None;
    }
    let outline_c = outline.get();
    if outline_c != Color::TRANSPARENT {
        return Some(if hovered {
            shared::ink_on(outline_c)
        } else {
            outline_c
        });
    }
    let fill_c = fill.get();
    if fill_c != Color::TRANSPARENT {
        return Some(shared::ink_on(fill_c));
    }
    Some(shared::on_accent())
}

#[cfg(test)]
#[path = "button_test.rs"]
mod tests;
