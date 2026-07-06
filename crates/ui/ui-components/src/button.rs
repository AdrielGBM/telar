use std::rc::Rc;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::signal;
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke, TextStyle};
use theme_core::use_widget_theme;
use ui_core::{LayoutItem, StyledContainer, Text, WidgetCtx, box_item};

/// Padding a button reserves around its label, and the label's font size / corner radius. `Text::auto`
/// measures the label at its full line box (taller than a bare `font_size * line_height`), so the vertical
/// padding is lighter than the horizontal to keep the button close to its previous height.
const PAD_X: f32 = 14.0;
const PAD_Y: f32 = 6.0;
const FONT_SIZE: f32 = 14.0;
const RADIUS: f32 = 4.0;

/// A labelled, pressable button. This is the high-level convenience over the primitives (`box` +
/// `on_press` + `hover` + a centred `text`); it lives in `ui-components`, not the kernel, so an app can
/// drop it or ship its own. `fill`/`outline` are reactive colour closures (re-read every frame) so a
/// button styled from a theme token re-colours when the theme switches.
pub struct ButtonProps {
    pub label: &'static str,
    /// Filled variant colour. `Color::TRANSPARENT` (the default) means "unset" — the button keeps its
    /// theme-driven default fill. A closure so a theme token re-reads on every render.
    pub fill: Box<dyn Fn() -> Color>,
    /// Outlined variant colour; `Color::TRANSPARENT` means unset. Takes precedence only when `fill` is unset.
    pub outline: Box<dyn Fn() -> Color>,
    pub ghost: bool,
    pub on_press: Box<dyn Fn()>,
}

impl Default for ButtonProps {
    fn default() -> Self {
        Self {
            label: "",
            fill: Box::new(|| Color::TRANSPARENT),
            outline: Box::new(|| Color::TRANSPARENT),
            ghost: false,
            on_press: Box::new(|| {}),
        }
    }
}

pub fn button(ctx: &mut WidgetCtx, props: ButtonProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ButtonProps {
        label,
        fill,
        outline,
        ghost,
        on_press,
    } = props;
    // The reactive colour closures feed three independent style closures (base rect, hover rect, label
    // colour), so share them via `Rc` rather than move them into a single one.
    let fill: Rc<dyn Fn() -> Color> = Rc::from(fill);
    let outline: Rc<dyn Fn() -> Color> = Rc::from(outline);
    // The container tracks its own hover for the rect swap; the label's colour lives on a separate leaf,
    // so mirror the hover into this signal and read it from the label style (the outline variant flips
    // its text to white on hover).
    let hovered = signal(false);

    // The label must be a measured leaf (`Text::auto`) so it has intrinsic width inside the button's row;
    // a stretched `Text::new`/`single_line` would collapse to 0-wide and render nothing.
    let (label_fill, label_outline, label_hover) =
        (Rc::clone(&fill), Rc::clone(&outline), hovered.clone());
    let label_widget = Text::auto(
        ctx,
        move || label.to_string(),
        LayoutStyle::new(),
        move || {
            TextStyle::new(
                FONT_SIZE,
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
        ctx,
        // A row so the label's measured width sets the box's main-axis size (a column would collapse the
        // cross axis: `Text::auto` sets `align_self_stretch`, which fights content-sizing and renders 0-wide).
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_horizontal(PAD_X)
            .padding_vertical(PAD_Y),
        move |_r| variant_rect(base_fill.as_ref(), base_outline.as_ref(), ghost, false),
        vec![box_item(label_widget)],
    )?
    .on_hover_style(move |_r| {
        variant_rect(hover_fill.as_ref(), hover_outline.as_ref(), ghost, true)
    })
    .on_hover(move |h| hovered.set(h))
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
    let radius = BorderRadius::all(RADIUS);
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
                .with_stroke(Stroke::new(outline_c, 1.5))
                .with_radius(radius)
        };
    }
    let fill_c = fill();
    if fill_c != Color::TRANSPARENT {
        return RectStyle::default().with_fill(fill_c).with_radius(radius);
    }
    let primary = use_widget_theme()
        .map(|t| t.widget_primary())
        .unwrap_or(Color::rgba(0.24, 0.47, 0.98, 1.0));
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
        return Color::rgba(0.15, 0.15, 0.2, 1.0);
    }
    let outline_c = outline();
    if outline_c != Color::TRANSPARENT {
        return if hovered { Color::WHITE } else { outline_c };
    }
    if fill() != Color::TRANSPARENT {
        return Color::WHITE;
    }
    use_widget_theme()
        .map(|t| t.widget_on_primary())
        .unwrap_or(Color::WHITE)
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
        let mut ctx = WidgetCtx::new();
        let mut btn = button(
            &mut ctx,
            ButtonProps {
                label: "OK",
                fill: Box::new(|| Color::rgba(0.2, 0.4, 0.9, 1.0)),
                on_press: Box::new(move || sink.set(true)),
                ..Default::default()
            },
        )
        .unwrap();
        let node = btn.layout_node();
        let rect = track_layout(&ctx, node).unwrap();
        compute_layout(
            &mut ctx,
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
}
