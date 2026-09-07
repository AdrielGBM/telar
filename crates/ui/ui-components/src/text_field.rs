//! [`text_field`]: a bordered, labelled box around the bare `Input` primitive.

use std::rc::Rc;

use crate::shared;
use layout_core::{LayoutError, LayoutStyle};
use reactive_core::{Reactive, RwSignal, signal};
use renderer_core::{Border, BorderRadius, Color, RectStyle, ShapeStyle};
use telar_macros::Props;
use ui_core::{Children, Input, LayoutItem, StyledContainer, box_item, style_follows};

fn box_radius() -> f32 {
    shared::radius() * 2.0
}
fn pad_x() -> f32 {
    shared::spacing() * 1.5
}
fn pad_y() -> f32 {
    shared::spacing() * 1.25
}
const DEFAULT_WIDTH: f32 = 300.0;

fn line_box(text_size: f32) -> LayoutStyle {
    LayoutStyle::new().height(ui_core::single_line_box(text_size))
}
fn field_box(width: f32) -> LayoutStyle {
    LayoutStyle::new()
        .flex_column()
        .width(width)
        .padding_horizontal(pad_x())
        .padding_vertical(pad_y())
        // The border below is unconditional, and on a surface that draws one in whole cells it needs a cell to draw in — without it a field whose padding quantised away had its frame and its own text on the same row.
        .bordered()
}

/// A labelled, bordered text input: the `Input` primitive (kernel, unstyled) wrapped in a padded/rounded box (see the raw `box fill:surface_alt stroke:border radius:8 pad_x:12 pad_y:10 > input` pattern in `apps/sandbox/src/features/reactivity.rsx`), with an optional caption label stacked above it. High-level sugar; lives in `ui-components`, not the kernel, so an app can drop it or ship its own.
#[derive(Props)]
pub struct TextFieldProps {
    /// `None` (the default) makes the field uncontrolled: it owns an internal `signal(String::new())`. `Some` binds it to a caller-owned signal (a controlled field), like `button`'s reactive props.
    #[props(some, into, default)]
    pub value: Option<RwSignal<String>>,
    /// Muted text shown in the box in place of the `Input` while `value` is empty — see the module's `text_field` doc for the swap's focus limitation.
    #[props(into, default)]
    pub placeholder: Reactive<String>,
    /// A small caption stacked above the box; omitted entirely (no extra row) when empty.
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Box width in logical px. `0.0` (the default) means "unset" and resolves to `DEFAULT_WIDTH`.
    #[props(default)]
    pub width: f32,
    /// The entered text's colour. `Color::TRANSPARENT` (the default) means "unset", and leaves the field in whatever the region around it is written in. A closure (re-read every frame) so a theme token or `$signal` colour re-colours live, like `button`'s `fill`/`outline`.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Runs when Enter is pressed while the field is focused.
    #[props(some, default)]
    pub on_submit: Option<Rc<dyn Fn()>>,
}

/// Builds a `text_field`: a bordered/padded box around `ui_core::Input`, swapping in a muted placeholder muted hint via the `Input`'s own `placeholder` while the value is empty — the field stays a live, always-mounted `Input`, so it is tappable/typable from a cold start (no swapped-in `Text` that would refuse focus).
pub fn text_field(
    props: TextFieldProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let TextFieldProps {
        value,
        placeholder,
        label,
        width,
        color,
        on_submit,
    } = props;
    let value = value.unwrap_or_else(|| signal(String::new()));
    let width = if width > 0.0 { width } else { DEFAULT_WIDTH };

    // Always a live `Input` with a muted placeholder, so the field is typable from a cold start: a swapped-in placeholder `Text` takes no focus, leaving an empty field impossible to click into.
    let mut input = Input::declaring(value, LayoutStyle::new(), move |t| {
        let t = shared::control_text(t, 1.0);
        match color.get() {
            c if c == Color::TRANSPARENT => t,
            c => t.with_color(c),
        }
    })?
    .placeholder(placeholder.get());
    if let Some(cb) = on_submit {
        input = input.on_submit(move || cb());
    }
    // The input is a leaf, so its node's style is followed from the box that outlives it.
    let line_node = input.layout_node();
    let field = box_item(input);

    let box_ = StyledContainer::new(
        field_box(width),
        |_r| {
            RectStyle::default()
                .with_fill(shared::surface_alt())
                .with_border(Border::uniform(shared::border(), 1.0))
                .with_radius(BorderRadius::all(box_radius()))
        },
        vec![box_item(field)],
    )?
    .styled_by(move || field_box(width));
    style_follows(line_node, move || {
        line_box(shared::control_text_size(line_node, 1.0))
    });

    shared::captioned(box_item(box_), label, width)
}

#[cfg(test)]
#[path = "text_field_test.rs"]
mod tests;
