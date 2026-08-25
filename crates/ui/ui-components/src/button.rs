use std::rc::Rc;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::signal;
use renderer_core::{Border, BorderRadius, Color, RectStyle, ShapeStyle, TextStyle};
use ui_core::focus::Role;
use ui_core::{LayoutItem, StyledContainer, Text, box_item};

use crate::shared;
use crate::shared::props_default;

/// Padding a button reserves around its label, derived from the theme's spacing unit rather than fixed so one
/// theme number moves it. `Text::new` measures the label at its full line box (taller than a bare
/// `font_size * line_height`), which is why the vertical share is the lighter of the two.
fn pad_x() -> f32 {
    shared::spacing() * 1.75
}
fn pad_y() -> f32 {
    shared::spacing() * 0.75
}

/// A row so the label's measured width sets the box's main-axis size (a column would collapse the cross axis:
/// `Text::new` sets `align_self_stretch`, which fights content-sizing and renders 0-wide).
fn shell() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_horizontal(pad_x())
        .padding_vertical(pad_y())
}

/// A labelled, pressable button. This is the high-level convenience over the primitives (`box` +
/// `on_press` + `hover` + a centred `text`); it lives in `ui-components`, not the kernel, so an app can
/// drop it or ship its own. `fill`/`outline` are reactive colour closures (re-read every frame) so a
/// button styled from a theme token re-colours when the theme switches.
pub struct ButtonProps {
    pub label: Box<dyn Fn() -> String>,
    /// Filled variant colour. `Color::TRANSPARENT` (the default) means "unset" — the button keeps its
    /// theme-driven default fill. A closure so a theme token re-reads on every render.
    pub fill: Box<dyn Fn() -> Color>,
    /// Outlined variant colour; `Color::TRANSPARENT` means unset. Takes precedence only when `fill` is unset.
    pub outline: Box<dyn Fn() -> Color>,
    pub ghost: bool,
    pub on_press: Box<dyn Fn()>,
}

props_default!(ButtonProps {
    label: text,
    fill: color,
    outline: color,
    ghost: (false),
    on_press: action,
});

pub fn button(props: ButtonProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ButtonProps {
        label,
        fill,
        outline,
        ghost,
        on_press,
    } = props;
    // The reactive colour closures feed three independent style closures (base rect, hover rect, label
    // colour), so share them via `Rc` rather than move them into a single one.
    let fill: shared::ReactiveColor = Rc::from(fill);
    let outline: shared::ReactiveColor = Rc::from(outline);
    // The container tracks its own hover for the rect swap; the label's colour lives on a separate leaf,
    // so mirror the hover into this signal and read it from the label style (the outline variant flips
    // its text to white on hover).
    let hovered = signal(false);

    // The label must be a measured leaf (`Text::new`) so it has intrinsic width inside the button's row;
    // a stretched `Text::new`/`single_line` would collapse to 0-wide and render nothing.
    let (label_fill, label_outline, label_hover) =
        (Rc::clone(&fill), Rc::clone(&outline), hovered.clone());
    let label_widget = Text::new(
        move || label(),
        LayoutStyle::new(),
        move || {
            TextStyle::new(
                shared::font_size(),
                label_color(
                    label_fill.as_ref(),
                    label_outline.as_ref(),
                    ghost,
                    label_hover.get(),
                ),
            )
        },
    )?;

    let (base_fill, base_outline) = (Rc::clone(&fill), Rc::clone(&outline));
    let (hover_fill, hover_outline) = (Rc::clone(&fill), Rc::clone(&outline));
    let container = StyledContainer::new(
        shell(),
        move |_r| variant_rect(base_fill.as_ref(), base_outline.as_ref(), ghost, false),
        vec![box_item(label_widget)],
    )?
    .styled_by(shell)
    .hover_style(move |_r| variant_rect(hover_fill.as_ref(), hover_outline.as_ref(), ghost, true))
    .on_hover(move |h| hovered.set(h))
    .control(Role::Button)
    .on_press(on_press);
    Ok(box_item(container))
}

/// Resolves the box paint for the current frame and hover state from the variant inputs, re-reading the
/// reactive `fill`/`outline` closures so a theme switch re-colours the button. Mirrors the old `ButtonStyle`:
/// ghost is transparent, outline strokes then fills on hover, filled keeps its fill, and the no-variant
/// default is the theme's primary (darkened on hover).
fn variant_rect(
    fill: &dyn Fn() -> Color,
    outline: &dyn Fn() -> Color,
    ghost: bool,
    hovered: bool,
) -> RectStyle {
    let radius = BorderRadius::all(shared::radius());
    if ghost {
        return RectStyle::default().with_radius(radius);
    }
    let outline_c = outline();
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
    let fill_c = fill();
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

/// The label colour for the current frame and hover state, mirroring the old `ButtonStyle` text/text_hover:
/// ghost is a dark neutral, outline is its own colour (white on hover), filled is white, and the no-variant
/// default is the theme's on-primary.
fn label_color(
    fill: &dyn Fn() -> Color,
    outline: &dyn Fn() -> Color,
    ghost: bool,
    hovered: bool,
) -> Color {
    if ghost {
        return shared::ink();
    }
    let outline_c = outline();
    if outline_c != Color::TRANSPARENT {
        return if hovered {
            shared::ink_on(outline_c)
        } else {
            outline_c
        };
    }
    let fill_c = fill();
    if fill_c != Color::TRANSPARENT {
        return shared::ink_on(fill_c);
    }
    shared::on_accent()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerButton, PointerSource};
    use ui_core::{Component, compute_layout, track_layout};

    use super::*;

    // A tap (press then release inside) fires on_press; press alone does not.
    #[test]
    fn tap_fires_on_press() {
        let flag = Rc::new(Cell::new(false));
        let sink = flag.clone();
        crate::test_support::fresh_layout_runtime();
        let mut btn = button(ButtonProps {
            label: Box::new(|| "OK".to_string()),
            fill: Box::new(|| Color::rgba(0.2, 0.4, 0.9, 1.0)),
            on_press: Box::new(move || sink.set(true)),
            ..Default::default()
        })
        .unwrap();
        let node = btn.layout_node();
        let rect = track_layout(node).unwrap();
        compute_layout(
            node,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(80.0),
        )
        .unwrap();

        let r = rect.get();
        let (cx, cy) = ((r.x + r.width / 2.0) as f64, (r.y + r.height / 2.0) as f64);
        btn.on_event(&Event::PointerPressed {
            x: cx,
            y: cy,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        assert!(!flag.get(), "press alone must not fire on_press");
        btn.on_event(&Event::PointerReleased {
            x: cx,
            y: cy,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        assert!(flag.get(), "a tap fires on_press");
    }

    /// A theme switch has to re-*space*, not only re-colour.
    ///
    /// Paint is a closure the renderer re-runs every frame, so a colour token switches for free. A metric is a
    /// number handed to the layout tree once, when the node is made — so until [`ui_core::style_follows`] the
    /// button kept the padding of the theme it was built under, and only a rebuild caught it up.
    #[test]
    fn switching_theme_re_spaces_the_button() {
        use std::any::Any;

        use theme_core::{Theme, ThemeTokens, set_theme};
        use ui_core::relayout_if_dirty;

        #[derive(Clone)]
        struct Spaced(f32);
        impl Theme for Spaced {
            fn as_any(&self) -> &dyn Any {
                self
            }
        }
        impl ThemeTokens for Spaced {
            fn spacing(&self) -> f32 {
                self.0
            }
        }

        crate::test_support::fresh_layout_runtime();
        set_theme(Spaced(8.0));
        // Measured with no label, so the width is the padding and nothing else — what a font system made of
        // the text is a different question from whether the box followed the theme.
        let btn = button(ButtonProps::default()).unwrap();
        let node = btn.layout_node();
        let rect = track_layout(node).unwrap();
        compute_layout(node, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
        assert_eq!(
            rect.get().width,
            2.0 * 8.0 * 1.75,
            "a button starts at the padding of the theme it was built under"
        );

        set_theme(Spaced(24.0));
        relayout_if_dirty();
        assert_eq!(
            rect.get().width,
            2.0 * 24.0 * 1.75,
            "and follows the theme it is switched to, without being rebuilt"
        );
    }

    /// The control size is one ambient number that every component interprets through its own proportions —
    /// the alternative being a `size` prop on each of them and a table of what each of their parts measures at
    /// each size. So the check is that the button did *not* need to know: nothing about it mentions a size, and
    /// it still gets smaller.
    #[test]
    fn the_ambient_control_size_scales_a_control_that_never_asked_for_one() {
        use std::any::Any;

        use theme_core::{ControlSize, Theme, ThemeTokens, set_control_size, set_theme};
        use ui_core::relayout_if_dirty;

        #[derive(Clone)]
        struct Plain;
        impl Theme for Plain {
            fn as_any(&self) -> &dyn Any {
                self
            }
        }
        impl ThemeTokens for Plain {
            fn spacing(&self) -> f32 {
                8.0
            }
        }

        crate::test_support::fresh_layout_runtime();
        set_theme(Plain);
        set_control_size(ControlSize::Regular);
        let btn = button(ButtonProps::default()).unwrap();
        let node = btn.layout_node();
        let rect = track_layout(node).unwrap();
        compute_layout(node, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
        let regular = rect.get().width;

        set_control_size(ControlSize::Mini);
        relayout_if_dirty();
        assert_eq!(
            rect.get().width,
            regular * ControlSize::Mini.scale(),
            "a denser control size carries through the button's own padding ratio"
        );
        set_control_size(ControlSize::Regular);
    }
}
