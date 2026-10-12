//! [`text_field`]: a bordered, labelled box around the bare `Input` primitive.

use std::rc::Rc;

use crate::shared;
use telar::{
    Accessible, Border, BorderRadius, Children, Color, Input, Key, LayoutError, LayoutItem,
    LayoutStyle, ModifiersState, Props, Reactive, RectStyle, RwSignal, ShapeStyle, StyledContainer,
    SurfaceAmendment, amend_surface, box_item, focus::FocusId, signal, style_follows,
};

/// A field's width in ems when neither its call nor the theme names one: room for a sentence-long value.
const WIDTH_EMS: f32 = 21.5;

/// A text field's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the field paints and lays out, so a theme that moves the role still moves every field it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextFieldTokens {
    /// Corner radius. `None` is the theme's `radius_control`.
    pub radius: Option<f32>,
    /// Space between the frame and the text at the regular control size, scaled with the control size in force. `None` is [`metrics::control_pad_x`](crate::metrics::control_pad_x).
    pub pad_x: Option<f32>,
    /// Width of the frame. `None` is the theme's `border_width`.
    pub border_width: Option<f32>,
    /// Width of a field whose call names none, at the regular control size, scaled with the control size in force. `None` is 21.5 ems of the document's text.
    pub width: Option<f32>,
}

impl TextFieldTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_control)
    }

    pub fn pad_x(&self) -> f32 {
        self.pad_x
            .map_or_else(shared::control_pad_x, |pad| pad * telar::control_scale())
    }

    pub fn border_width(&self) -> f32 {
        self.border_width.unwrap_or_else(shared::border_width)
    }

    pub fn width(&self) -> f32 {
        shared::width_or_measure(self.width, WIDTH_EMS)
    }
}

/// The input is the whole height of the field, so the press and the audit see the whole field rather than the bare line inside it, and the line is centred in it.
fn line_box() -> LayoutStyle {
    LayoutStyle::new().height(shared::control_height())
}
fn field_box(width: f32) -> LayoutStyle {
    LayoutStyle::new()
        .flex_column()
        .width(width)
        .max_width(telar::SizeDimension::Percent(1.0))
        .padding_horizontal(TextFieldTokens::current().pad_x())
        // The border below is unconditional, and on a surface that draws one in whole cells it needs a cell to draw in — without it a field whose padding quantised away had its frame and its own text on the same row.
        .bordered()
}

/// A labelled, bordered text input: the `Input` primitive (kernel, unstyled) in a framed box one control tall, with an optional caption stacked above it, outside the box. High-level sugar; lives in `telar-components`, not the kernel, so an app can drop it or ship its own.
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
    /// Box width in logical px. `0.0` (the default) means "unset" and resolves to [`TextFieldTokens::width`].
    #[props(default)]
    pub width: f32,
    /// The entered text's colour. `Color::TRANSPARENT` (the default) means "unset", and leaves the field in whatever the region around it is written in. A closure (re-read every frame) so a theme token or `$signal` colour re-colours live, like `button`'s `fill`/`outline`.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Runs when Enter is pressed while the field is focused.
    #[props(some, default)]
    pub on_submit: Option<Rc<dyn Fn()>>,
    /// Hears every key pressed while the field holds the keyboard, before the field acts on it; answering `true` takes the key from the field. What a field that walks a list of suggestions under it needs: the arrows and Enter are keys a focused field keeps for itself.
    #[props(some, default)]
    pub on_key: Option<FieldKeyHandler>,
    /// The suggestion a field like that has its cursor on, so a reader announces it while the keyboard stays in the field. `None` (the default) points at nothing.
    #[props(some, default)]
    pub active_descendant: Option<Rc<dyn Fn() -> Option<FocusId>>>,
    /// The list of suggestions such a field opens, so a reader is told where they are and the row [`active_descendant`](Self::active_descendant) names is looked for in it. `None` (the default) names none.
    #[props(some, default)]
    pub controls: Option<Rc<dyn Fn() -> Option<FocusId>>>,
    /// Amends the paint of the framed box — the field's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`TextFieldTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// What [`TextFieldProps::on_key`] is called with: the key and the modifiers held with it.
pub type FieldKeyHandler = Rc<dyn Fn(&Key, ModifiersState) -> bool>;

/// Builds a `text_field`: a bordered/padded box around `telar::Input`, swapping in a muted placeholder muted hint via the `Input`'s own `placeholder` while the value is empty — the field stays a live, always-mounted `Input`, so it is tappable/typable from a cold start (no swapped-in `Text` that would refuse focus).
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
        on_key,
        active_descendant,
        controls,
        style,
    } = props;
    let value = value.unwrap_or_else(|| signal(String::new()));
    let width = move || {
        if width > 0.0 {
            width
        } else {
            TextFieldTokens::current().width()
        }
    };

    // Always a live `Input` with a muted placeholder, so the field is typable from a cold start: a swapped-in placeholder `Text` takes no focus, leaving an empty field impossible to click into.
    let mut input = Input::declaring(value, LayoutStyle::new(), move |t| {
        let t = shared::control_text(t, 1.0);
        match color.get() {
            c if c == Color::TRANSPARENT => t,
            c => t.with_color(c),
        }
    })?
    .placeholder(placeholder.get())
    .a11y_label({
        let label = label.clone();
        move || match label.get() {
            caption if caption.is_empty() => placeholder.get(),
            caption => caption,
        }
    });
    if let Some(cb) = on_submit {
        input = input.on_submit(move || cb());
    }
    if let Some(hook) = on_key {
        input = input.on_key(move |key, modifiers| hook(key, modifiers));
    }
    if let Some(read) = active_descendant {
        telar::focus::set_active_descendant(input.focus_id(), move || read());
    }
    if let Some(read) = controls {
        telar::focus::set_controls(input.focus_id(), move || read());
    }
    // The input is a leaf, so its node's style is followed from the box that outlives it.
    let line_node = input.layout_node();
    let line_focus = input.focus_id();
    let field = box_item(input);

    let box_ = StyledContainer::new(
        field_box(width()),
        move |_r| amend_surface(frame(), &style),
        vec![box_item(field)],
    )?
    .frames_focus_of(line_focus)
    .styled_by(move || field_box(width()));
    style_follows(line_node, line_box);

    shared::captioned(box_item(box_), label, Reactive::of(width))
}

/// The box's paint before the caller's amendment.
fn frame() -> RectStyle {
    let tokens = TextFieldTokens::current();
    RectStyle::default()
        .with_fill(shared::surface_alt())
        .with_border(Border::uniform(shared::border(), tokens.border_width()))
        .with_radius(BorderRadius::all(tokens.radius()))
}

#[cfg(test)]
#[path = "text_field_test.rs"]
mod tests;
