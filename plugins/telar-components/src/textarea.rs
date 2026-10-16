//! [`textarea`]: a framed, labelled box of several lines around the bare `TextArea` primitive, drawn in the same frame as a text field.

use telar::{
    Accessible, Children, Color, Container, JustifyContent, LayoutError, LayoutItem,
    LayoutScrollArea, LayoutStyle, NodeId, Props, Reactive, RwSignal, StyledContainer,
    SurfaceAmendment, Text, TextArea, TextWrap, amend_surface, box_item, children, signal,
    style_follows, use_theme_tokens,
};

use crate::shared;
use crate::strings;
use crate::text_field::{field_box, field_width, frame};

/// How many lines a text area shows when its call names none.
const ROWS: u32 = 3;

/// Several lines of text typed into a framed box, with an optional caption above it and an optional count of what was typed under it.
///
/// Drawn in a text field's frame and sized by [`TextFieldTokens`](crate::TextFieldTokens), so the two stand together in a form. It is `rows` lines tall, grows with the rows what is typed takes up to `max_rows`, and scrolls past that, keeping the row being typed on in view. A line longer than the box wraps onto the next row, between words and inside a word too long for one, as an HTML `<textarea>` does; `text_wrap: TextWrap::NoWrap` breaks lines only where Enter does and scrolls a longer one sideways instead.
#[derive(Props)]
pub struct TextareaProps {
    /// `None` (the default) makes the area uncontrolled: it owns an internal `signal(String::new())`. `Some` binds it to a caller-owned signal.
    #[props(some, into, default)]
    pub value: Option<RwSignal<String>>,
    /// Muted text shown in the area while `value` is empty, and what a reader calls it when it has no `label`.
    #[props(into, default)]
    pub placeholder: Reactive<String>,
    /// A small caption stacked above the box, and what a reader calls the area; omitted entirely when empty.
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Box width in logical px. `0.0` (the default) is [`TextFieldTokens::width`](crate::TextFieldTokens::width).
    #[props(default)]
    pub width: f32,
    /// How many lines tall the area is with nothing in it. Three by default.
    #[props(default = ROWS)]
    pub rows: u32,
    /// How many lines tall it grows to as lines are typed before it scrolls. `0` (the default) keeps it at `rows`.
    #[props(default)]
    pub max_rows: u32,
    /// Whether a line longer than the box wraps onto the next row ([`TextWrap::Wrap`], the default) or stays whole and scrolls sideways.
    #[props(default)]
    pub text_wrap: TextWrap,
    /// Shows how many characters were typed, under the box. Off by default.
    #[props(default)]
    pub counter: bool,
    /// The count the counter measures against, shown beside it and drawn in the theme's error colour once it is passed. `0` (the default) names no limit. Typing past it is not refused: the counter says so, and the form decides.
    #[props(default)]
    pub max_length: usize,
    /// The entered text's colour. `Color::TRANSPARENT` (the default) leaves it in whatever the region around it is written in.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Amends the paint of the framed box — the area's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`TextFieldTokens`](crate::TextFieldTokens) is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A framed, labelled box of several lines.
pub fn textarea(
    props: TextareaProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let TextareaProps {
        value,
        placeholder,
        label,
        width,
        rows,
        max_rows,
        text_wrap,
        counter,
        max_length,
        color,
        style,
    } = props;
    let value = value.unwrap_or_else(|| signal(String::new()));
    let rows = rows.max(1);
    let max_rows = max_rows.max(rows);
    let width = move || field_width(width);

    let area = TextArea::declaring(value, LayoutStyle::new(), move |t| {
        let t = shared::control_text(t, 1.0);
        match color.get() {
            c if c == Color::TRANSPARENT => t,
            c => t.with_color(c),
        }
    })?
    .placeholder(placeholder.get())
    .text_wrap(text_wrap)
    .a11y_label({
        let label = label.clone();
        move || match label.get() {
            caption if caption.is_empty() => placeholder.get(),
            caption => caption,
        }
    });
    let line_focus = area.focus_id();
    let line_node = area.layout_node();
    let line = move || line_height(line_node);
    // The area fills the view however few lines it holds, so a press anywhere in the view lands in it.
    style_follows(line_node, move || {
        LayoutStyle::new().min_height(rows as f32 * line())
    });

    let view = move || {
        LayoutStyle::new()
            .min_height(rows as f32 * line())
            .max_height(max_rows as f32 * line())
    };
    let scroll = LayoutScrollArea::new(view(), box_item(area))?;
    style_follows(scroll.layout_node(), view);

    let framed = move || field_box(width()).padding_vertical(pad_y(line_node));
    let framed_box = StyledContainer::new(
        framed(),
        move |_r| amend_surface(frame(), &style),
        children![scroll],
    )?
    .frames_focus_of(line_focus)
    .styled_by(framed);

    let field = shared::captioned(box_item(framed_box), label, Reactive::of(width))?;
    if !counter {
        return Ok(field);
    }
    let count = counted(value, max_length, width)?;
    let column = Container::new(
        LayoutStyle::column().gap(shared::gap_xs()),
        children![field, count],
    )?;
    Ok(box_item(column))
}

/// The height of one line the area draws, in the text style in force where it is.
fn line_height(node: NodeId) -> f32 {
    telar::line_box(&shared::control_text(
        telar::inherited_text_style(node),
        1.0,
    ))
}

/// The space above and below the lines, so an area one line tall is as tall as a text field beside it.
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

#[cfg(test)]
#[path = "textarea_test.rs"]
mod tests;
