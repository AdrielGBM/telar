//! [`text_field`]: a bordered, labelled box around the bare `Input` primitive, or around a `TextArea` when it holds several lines.

use std::rc::Rc;

use crate::shared;
use crate::strings;
use telar::{
    Accessible, AlignItems, Border, BorderRadius, Children, Color, Container, Input,
    JustifyContent, Key, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, ModifiersState,
    NodeId, Props, Reactive, RectStyle, RwSignal, ShapeStyle, StyledContainer, SurfaceAmendment,
    Text, TextArea, TextStyle, TextWrap, amend_surface, box_item, children, focus::FocusId, signal,
    style_follows, use_theme_tokens,
};

/// A field's width in ems when neither its call nor the theme names one: room for a sentence-long value.
const WIDTH_EMS: f32 = 21.5;

/// How many lines a field of several lines shows when its call names none.
const ROWS: u32 = 3;

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

/// A field's width: `width` where its call names one, else [`TextFieldTokens::width`].
pub(crate) fn field_width(width: f32) -> f32 {
    if width > 0.0 {
        width
    } else {
        TextFieldTokens::current().width()
    }
}

/// The input is the whole height of the field, so the press and the audit see the whole field rather than the bare line inside it, and the line is centred in it.
fn line_box() -> LayoutStyle {
    LayoutStyle::new().height(shared::control_height())
}

/// The framed box every catalogue field is drawn in — a text field of one line or of several, a search field — before what it holds is arranged in it: `width` wide, never wider than its parent, and padded from the frame by [`TextFieldTokens::pad_x`].
pub(crate) fn field_box(width: f32) -> LayoutStyle {
    LayoutStyle::column()
        .width(width)
        .max_width(telar::SizeDimension::Percent(1.0))
        .padding_horizontal(TextFieldTokens::current().pad_x())
        // The border below is unconditional, and on a surface that draws one in whole cells it needs a cell to draw in — without it a field whose padding quantised away had its frame and its own text on the same row.
        .bordered()
}

/// A labelled, bordered field for text, with an optional caption stacked above it, outside the box, and an optional count of what was typed under it. High-level sugar; lives in `telar-components`, not the kernel, so an app can drop it or ship its own.
///
/// One line by default: the `Input` primitive in a framed box one control tall, where Enter runs [`on_submit`](Self::on_submit) and a value longer than the box scrolls sideways under it, keeping the caret in view. [`multiline`](Self::multiline) makes it several lines in the same frame, around the `TextArea` primitive: `rows` lines tall, growing with the rows what is typed takes up to `max_rows` and scrolling past that with the caret kept in view; Enter breaks the line, and a line longer than the box wraps onto the next row as an HTML `<textarea>` does, unless `text_wrap` says otherwise.
#[derive(Props)]
pub struct TextFieldProps {
    /// `None` (the default) makes the field uncontrolled: it owns an internal `signal(String::new())`. `Some` binds it to a caller-owned signal (a controlled field), like `button`'s reactive props.
    #[props(some, into, default)]
    pub value: Option<RwSignal<String>>,
    /// Muted text shown in the box while `value` is empty, and what a reader calls the field when it has no `label`.
    #[props(into, default)]
    pub placeholder: Reactive<String>,
    /// A small caption stacked above the box, and what a reader calls the field; omitted entirely (no extra row) when empty.
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Box width in logical px. `0.0` (the default) means "unset" and resolves to [`TextFieldTokens::width`].
    #[props(default)]
    pub width: f32,
    /// Several lines rather than one: Enter breaks the line instead of submitting, and the box is `rows` lines tall. Off by default.
    #[props(default)]
    pub multiline: bool,
    /// How many lines tall a [`multiline`](Self::multiline) field is with nothing in it. Three by default.
    #[props(default = ROWS)]
    pub rows: u32,
    /// How many lines tall a [`multiline`](Self::multiline) field grows to as lines are typed before it scrolls. `0` (the default) keeps it at `rows`.
    #[props(default)]
    pub max_rows: u32,
    /// Whether a line longer than a [`multiline`](Self::multiline) field wraps onto the next row ([`TextWrap::Wrap`], the default) or stays whole and scrolls sideways. A field of one line never wraps.
    #[props(default)]
    pub text_wrap: TextWrap,
    /// Shows how many characters were typed, under the box. Off by default.
    #[props(default)]
    pub counter: bool,
    /// The count the counter measures against, shown beside it and drawn in the theme's error colour once it is passed. `0` (the default) names no limit. Typing past it is not refused: the counter says so, and the form decides.
    #[props(default)]
    pub max_length: usize,
    /// The entered text's colour. `Color::TRANSPARENT` (the default) means "unset", and leaves the field in whatever the region around it is written in. A closure (re-read every frame) so a theme token or `$signal` colour re-colours live, like `button`'s `fill`/`outline`.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Runs when Enter is pressed while a field of one line is focused. A [`multiline`](Self::multiline) field breaks the line instead.
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
    /// Receives the id the editor typed into holds in the tab order, once the field exists, and is emptied when the field goes. For the caller that has to take the keyboard to the field later, as `focus_id:$sig` does on an `input`.
    #[props(some, into, default)]
    pub focus_id: Option<RwSignal<Option<FocusId>>>,
}

/// What [`TextFieldProps::on_key`] is called with: the key and the modifiers held with it.
pub type FieldKeyHandler = Rc<dyn Fn(&Key, ModifiersState) -> bool>;

/// Builds a `text_field`: a bordered/padded box around `telar::Input`, or around `telar::TextArea` in a scroll when it is [`multiline`](TextFieldProps::multiline). The muted placeholder is the editor's own, drawn while the value is empty, so the field stays a live, always-mounted editor — tappable/typable from a cold start, with no swapped-in `Text` that would refuse focus.
pub fn text_field(
    props: TextFieldProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    adorned_field(props, Adornments::default()).map(|(field, _)| field)
}

/// What a field draws in its frame beside the line typed into: a search field's glass before it, and its clear button after it.
#[derive(Default)]
pub(crate) struct Adornments {
    pub(crate) leading: Option<Box<dyn LayoutItem>>,
    pub(crate) trailing: Option<Box<dyn LayoutItem>>,
}

/// The editor a field frames, and what the frame has to know about it.
struct Editor {
    item: Box<dyn LayoutItem>,
    focus: FocusId,
    /// The editor's own node, whose text style the frame's padding is measured from.
    node: NodeId,
    /// The editor's style as the field lays it out, followed from the frame that outlives the leaf.
    layout: Box<dyn Fn() -> LayoutStyle>,
}

/// A text field with `adornments` in its frame, laid along the line, and the id the editor holds focus by.
pub(crate) fn adorned_field(
    props: TextFieldProps,
    adornments: Adornments,
) -> Result<(Box<dyn LayoutItem>, FocusId), LayoutError> {
    let TextFieldProps {
        value,
        placeholder,
        label,
        width,
        multiline,
        rows,
        max_rows,
        text_wrap,
        counter,
        max_length,
        color,
        on_submit,
        on_key,
        active_descendant,
        controls,
        style,
        focus_id,
    } = props;
    let value = value.unwrap_or_else(|| signal(String::new()));
    let width = move || field_width(width);
    let text = move |t: TextStyle| {
        let t = shared::control_text(t, 1.0);
        match color.get() {
            c if c == Color::TRANSPARENT => t,
            c => t.with_color(c),
        }
    };
    let name = {
        let (label, placeholder) = (label.clone(), placeholder.clone());
        move || match label.get() {
            caption if caption.is_empty() => placeholder.get(),
            caption => caption,
        }
    };
    let Adornments { leading, trailing } = adornments;
    let adorned = leading.is_some() || trailing.is_some();
    let editor = match multiline {
        false => single_line(value, &placeholder, name, text, on_submit, on_key, adorned)?,
        true => {
            let lines = Lines {
                rows: rows.max(1),
                max_rows: max_rows.max(rows.max(1)),
                wrap: text_wrap,
            };
            several_lines(value, &placeholder, name, text, on_key, lines)?
        }
    };
    let Editor {
        item,
        focus: line_focus,
        node: line_node,
        layout,
    } = editor;
    if let Some(read) = active_descendant {
        telar::focus::set_active_descendant(line_focus, move || read());
    }
    if let Some(read) = controls {
        telar::focus::set_controls(line_focus, move || read());
    }
    if let Some(published) = focus_id {
        published.set(Some(line_focus));
        telar::on_cleanup(move || published.set(None));
    }
    let mut content: Vec<Box<dyn LayoutItem>> = leading.into_iter().collect();
    content.push(item);
    content.extend(trailing);

    let arranged = move || {
        let framed = match multiline {
            true => field_box(width()).padding_vertical(pad_y(line_node)),
            false => field_box(width()),
        };
        match (adorned, multiline) {
            (false, _) => framed,
            (true, multiline) => framed
                .flex_row()
                .align_items(match multiline {
                    true => AlignItems::START,
                    false => AlignItems::CENTER,
                })
                .gap(shared::gap_sm()),
        }
    };
    let box_ = StyledContainer::new(
        arranged(),
        move |_r| amend_surface(frame(), &style),
        content,
    )?
    .frames_focus_of(line_focus)
    .styled_by(arranged);
    style_follows(line_node, layout);

    let field = shared::captioned(box_item(box_), label, Reactive::of(width))?;
    if !counter {
        return Ok((field, line_focus));
    }
    let count = counted(value, max_length, width)?;
    let column = Container::new(
        LayoutStyle::column().gap(shared::gap_xs()),
        children![field, count],
    )?;
    Ok((box_item(column), line_focus))
}

/// The field of one line: an `Input` the whole height of the box.
fn single_line(
    value: RwSignal<String>,
    placeholder: &Reactive<String>,
    name: impl Fn() -> String + 'static,
    text: impl Fn(TextStyle) -> TextStyle + 'static,
    on_submit: Option<Rc<dyn Fn()>>,
    on_key: Option<FieldKeyHandler>,
    adorned: bool,
) -> Result<Editor, LayoutError> {
    let mut input = Input::declaring(value, LayoutStyle::new(), text)?
        .placeholder(placeholder.get())
        .a11y_label(name);
    if let Some(submit) = on_submit {
        input = input.on_submit(move || submit());
    }
    if let Some(hook) = on_key {
        input = input.on_key(move |key, modifiers| hook(key, modifiers));
    }
    let (focus, node) = (input.focus_id(), input.layout_node());
    let layout = Box::new(move || match adorned {
        true => line_box().flex_grow(1.0).flex_shrink(1.0).min_width(0.0),
        false => line_box(),
    });
    Ok(Editor {
        item: box_item(input),
        focus,
        node,
        layout,
    })
}

/// How a field of several lines is sized and broken.
struct Lines {
    rows: u32,
    max_rows: u32,
    wrap: TextWrap,
}

/// The field of several lines: a `TextArea` filling a scroll `rows` lines tall that grows to `max_rows`, so a press anywhere in the view lands in it.
fn several_lines(
    value: RwSignal<String>,
    placeholder: &Reactive<String>,
    name: impl Fn() -> String + 'static,
    text: impl Fn(TextStyle) -> TextStyle + 'static,
    on_key: Option<FieldKeyHandler>,
    Lines {
        rows,
        max_rows,
        wrap,
    }: Lines,
) -> Result<Editor, LayoutError> {
    let mut area = TextArea::declaring(value, LayoutStyle::new(), text)?
        .placeholder(placeholder.get())
        .text_wrap(wrap)
        .a11y_label(name);
    if let Some(hook) = on_key {
        area = area.on_key(move |key, modifiers| hook(key, modifiers));
    }
    let (focus, node) = (area.focus_id(), area.layout_node());
    let line = move || line_height(node);
    let view = move || {
        LayoutStyle::new()
            .min_height(rows as f32 * line())
            .max_height(max_rows as f32 * line())
    };
    let scroll = LayoutScrollArea::new(view(), box_item(area))?;
    style_follows(scroll.layout_node(), view);
    let layout = Box::new(move || LayoutStyle::new().min_height(rows as f32 * line()));
    Ok(Editor {
        item: box_item(scroll),
        focus,
        node,
        layout,
    })
}

/// The height of one line the editor draws, in the text style in force where it is.
fn line_height(node: NodeId) -> f32 {
    telar::line_box(&shared::control_text(
        telar::inherited_text_style(node),
        1.0,
    ))
}

/// The space above and below the lines of a field of several, so one a single line tall is as tall as a field of one line beside it.
fn pad_y(node: NodeId) -> f32 {
    ((shared::control_height() - line_height(node)) / 2.0).max(0.0)
}

/// How many characters `value` holds, against `max` where there is one, at the end of a row as wide as the box, and in the theme's error colour once it is past `max`.
fn counted(
    value: RwSignal<String>,
    max: usize,
    width: impl Fn() -> f32 + Clone + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let count = move || value.with(|text| text.chars().count());
    let over = move || max > 0 && count() > max;
    let shown = move || match max {
        0 => count().to_string(),
        max => format!("{} / {max}", count()),
    };
    let said = move || {
        let n = count().to_string();
        match max {
            0 => strings::text_with(strings::CHARACTERS, &[("count", &n)]),
            max => strings::text_with(
                strings::CHARACTERS_OF,
                &[("count", &n), ("max", &max.to_string())],
            ),
        }
    };
    let text = Text::declaring(shown, LayoutStyle::new(), move |t| {
        let text = shared::quiet(t, shared::CAPTION_RATIO);
        if over() {
            text.with_color(use_theme_tokens().error())
        } else {
            text
        }
    })?
    .a11y_label(said);
    let row = move || {
        LayoutStyle::row()
            .width(width())
            .justify_content(JustifyContent::END)
    };
    let counter = Container::new(row(), children![text])?.styled_by(row);
    Ok(box_item(counter))
}

/// A field box's paint before the caller's amendment.
pub(crate) fn frame() -> RectStyle {
    let tokens = TextFieldTokens::current();
    RectStyle::default()
        .with_fill(shared::surface_alt())
        .with_border(Border::uniform(shared::border(), tokens.border_width()))
        .with_radius(BorderRadius::all(tokens.radius()))
}

#[cfg(test)]
#[path = "text_field_test.rs"]
mod tests;
