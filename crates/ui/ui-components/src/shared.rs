//! Internal helpers shared across the widget catalogue: the reactive-colour resolver, the common accent/surface
//! fallback constants, and the labelled-control row scaffold. Not part of the public API (`pub(crate)`).

use std::rc::Rc;

use layout_core::{AlignItems, LayoutError, LayoutStyle};
use renderer_core::{Color, TextStyle};
use ui_core::{Container, LayoutItem, Text, box_item};

/// A reactive colour prop re-erased to a shareable handle: a `Box<dyn Fn>` isn't `Clone`, but widgets must hand the same colour closure to several style closures, so they re-erase to this `Rc`.
pub(crate) type ReactiveColor = Rc<dyn Fn() -> Color>;

/// Fallback accent when no reactive `color` is supplied and no theme is active (matches `Button`'s default primary).
pub(crate) const DEFAULT_ACCENT: Color = Color::rgba(0.24, 0.47, 0.98, 1.0);
/// Opaque white surface fallback shared by `modal` and `drawer` when their `color` is unset.
pub(crate) const DEFAULT_SURFACE: Color = Color::rgba(1.0, 1.0, 1.0, 1.0);
/// Default text ink for the catalogue (labels, titles, values). `WidgetTheme` exposes no ink token, so
/// this is the shared fallback for text that isn't an accent.
pub(crate) const INK: Color = Color::rgba(0.15, 0.15, 0.2, 1.0);
/// Muted rail/track fallback shared by `slider`/`progress`/`spinner` when their `track_color` is unset.
pub(crate) const DEFAULT_TRACK: Color = Color::rgba(0.5, 0.5, 0.6, 0.3);

/// Resolve a reactive colour: `color()` unless it is `Color::TRANSPARENT` (the "unset" sentinel), else `fallback()`.
/// `color()` is evaluated once. Collapses the per-widget accent/track/surface/bubble resolvers into one shape.
pub(crate) fn resolve(color: &dyn Fn() -> Color, fallback: impl FnOnce() -> Color) -> Color {
    let c = color();
    if c == Color::TRANSPARENT {
        fallback()
    } else {
        c
    }
}

/// A control (checkbox box / radio ring / toggle pill) plus an optional label, laid out as one gap-10 row that
/// is itself the tap target: a tap runs `on_press`. Shared by checkbox, radio and toggle.
pub(crate) fn labelled_control(
    control: Box<dyn LayoutItem>,
    label: &str,
    on_press: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut children: Vec<Box<dyn LayoutItem>> = vec![box_item(control)];
    if !label.is_empty() {
        let label = label.to_string();
        // `auto` (measured leaf) so the label gets its intrinsic WIDTH in this row; a plain `Text::new` only stretches its cross-axis (height here), leaving width 0 and the label invisible.
        let text = Text::auto(
            move || label.clone(),
            LayoutStyle::new(),
            || TextStyle::new(14.0, INK),
        )?;
        children.push(box_item(text));
    }
    let row = Container::new(
        LayoutStyle::new()
            .flex_row()
            .gap(10.0)
            .align_items(AlignItems::CENTER),
        children,
    )?
    .on_press(on_press);
    Ok(box_item(row))
}
