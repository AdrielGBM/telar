use layout_core::LayoutError;
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke, TextStyle};
use ui_core::{Button, ButtonStyle, LayoutItem, WidgetCtx, box_item};

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
    let mut btn = Button::new(ctx, label)?;
    // Only override the widget's own theme default when a variant is actually requested.
    let has_variant = ghost || fill() != Color::TRANSPARENT || outline() != Color::TRANSPARENT;
    if has_variant {
        btn = btn.style(move || variant_style(fill.as_ref(), outline.as_ref(), ghost));
    }
    btn = btn.on_click(on_press);
    Ok(box_item(btn))
}

/// Resolves the `ButtonStyle` for the current frame from the variant inputs, re-reading the reactive
/// colour closures so a theme switch takes effect. Mirrors the styling the transpiler used to inline.
fn variant_style(
    fill: &dyn Fn() -> Color,
    outline: &dyn Fn() -> Color,
    ghost: bool,
) -> ButtonStyle {
    let radius = BorderRadius::all(4.0);
    if ghost {
        // Transparent in both states, dark neutral text.
        ButtonStyle {
            rect: RectStyle::default().with_radius(radius),
            rect_hover: RectStyle::default().with_radius(radius),
            text: TextStyle::new(14.0, Color::rgba(0.15, 0.15, 0.2, 1.0)),
            text_hover: TextStyle::new(14.0, Color::rgba(0.15, 0.15, 0.2, 1.0)),
        }
    } else {
        let outline = outline();
        if outline != Color::TRANSPARENT {
            ButtonStyle {
                rect: RectStyle::default()
                    .with_stroke(Stroke::new(outline, 1.5))
                    .with_radius(radius),
                rect_hover: RectStyle::default().with_fill(outline).with_radius(radius),
                text: TextStyle::new(14.0, outline),
                text_hover: TextStyle::new(14.0, Color::WHITE),
            }
        } else {
            let fill = fill();
            ButtonStyle {
                rect: RectStyle::default().with_fill(fill).with_radius(radius),
                rect_hover: RectStyle::default().with_fill(fill).with_radius(radius),
                text: TextStyle::new(14.0, Color::WHITE),
                text_hover: TextStyle::new(14.0, Color::WHITE),
            }
        }
    }
}
