//! [`code_view`]: read-only source in the token colours its caller worked out, with line numbers, marked lines and a copy button.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use telar::{
    Accessible, Border, BorderRadius, Canvas, Children, Color, Container, Declared, FontFamily,
    FontStyle, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, Props, Reactive, Rect,
    RectStyle, RenderNode, ShapeStyle, Span, StyledContainer, Text, TextStyle, TextWrap, Timer,
    box_item, focus::Role, inherited_text_style, line_box, memo, run_after, signal, track_layout,
};

use crate::line_gutter::LineGutter;
use crate::shared;
use crate::strings;
use crate::{IconButtonProps, icon_button};

const TAB_WIDTH: usize = 4;
/// Monospace's share of the text around it: a face whose every glyph is as wide as its widest reads larger than proportional type of the same size.
const MONO_RATIO: f32 = 0.92;
const MARK_ALPHA: f32 = 0.14;
const COPIED_FOR: Duration = Duration::from_millis(1500);

fn pad() -> f32 {
    shared::spacing()
}

/// What a run of source is, as far as its colour goes. The caller decides which runs are which; nothing here parses a language.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TokenKind {
    #[default]
    Plain,
    Keyword,
    Type,
    Function,
    String,
    Number,
    Comment,
    Punctuation,
    Attribute,
    /// A run the code is wrong about: what a compiler pointed at.
    Error,
}

/// A run of [`CodeViewProps::code`], by byte range, and what kind of token it is.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CodeSpan {
    pub range: Range<usize>,
    pub kind: TokenKind,
}

impl CodeSpan {
    pub fn new(range: Range<usize>, kind: TokenKind) -> Self {
        Self { range, kind }
    }
}

/// What [`CodeViewProps::token_style`] restyles a kind of token with.
pub type TokenStyler = Rc<dyn Fn(TokenKind) -> Declared>;

/// What [`CodeViewProps::on_copy`] is called with: the code put on the clipboard.
pub type CopyHandler = Rc<dyn Fn(&str)>;

/// Source shown as it is written, in a monospace face, with the runs its caller marked in their token colours: the source of a preview, a docs snippet, the excerpt a build error points into.
///
/// It reads nothing itself: [`spans`](Self::spans) carries whatever a lexer, a compiler's diagnostics or a hand-written table said about the text. Lines never wrap; a long one scrolls sideways.
#[derive(Props)]
pub struct CodeViewProps {
    #[props(into, default)]
    pub code: Reactive<String>,
    /// Runs of `code` by byte range. A run that does not start and end on a character boundary of `code` is left plain.
    #[props(into, default)]
    pub spans: Reactive<Vec<CodeSpan>>,
    /// The lines to mark, numbered as the gutter numbers them.
    #[props(into, default)]
    pub highlighted: Reactive<Vec<u32>>,
    /// The number of the first line, for an excerpt numbered as its file is. Default 1.
    #[props(default = 1)]
    pub first_line: u32,
    /// Shows the line numbers. On by default.
    #[props(default = true)]
    pub line_numbers: bool,
    /// Shows the button that copies the code. On by default.
    #[props(default = true)]
    pub copyable: bool,
    /// Fired with the code the copy button put on the clipboard.
    #[props(some, default)]
    pub on_copy: Option<CopyHandler>,
    /// What a reader calls the block, beyond its text.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
    /// The tallest the block grows, in logical px, before it scrolls. `0.0` (the default) grows to fit every line.
    #[props(default)]
    pub max_height: f32,
    /// Restyles each kind of token over the theme's own choice, for a palette of the caller's.
    #[props(some, default)]
    pub token_style: Option<TokenStyler>,
    /// The marked lines' wash. `Color::TRANSPARENT` (the default) falls back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
}

/// The theme's colour for each kind of token, read from the tokens every theme answers.
fn theme_token_style(kind: TokenKind) -> Declared {
    let tokens = telar::use_theme_tokens();
    match kind {
        TokenKind::Plain => Declared::default(),
        TokenKind::Keyword => Declared::default()
            .with_color(tokens.primary())
            .with_font_weight(600),
        TokenKind::Type => Declared::default().with_color(tokens.info()),
        TokenKind::Function => Declared::default().with_font_weight(600),
        TokenKind::String => Declared::default().with_color(tokens.success()),
        TokenKind::Number => Declared::default().with_color(tokens.warning()),
        TokenKind::Comment => Declared::default()
            .with_color(tokens.muted())
            .with_font_style(FontStyle::Italic),
        TokenKind::Punctuation => Declared::default().with_color(tokens.muted()),
        TokenKind::Attribute => Declared::default()
            .with_color(tokens.info())
            .with_font_style(FontStyle::Italic),
        TokenKind::Error => Declared::default()
            .with_color(tokens.error())
            .with_underline(true),
    }
}

/// The code as it is drawn: tabs set out as spaces, carriage returns and one closing newline dropped, and every run moved to where its text now sits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Prepared {
    pub text: String,
    pub runs: Vec<(Range<usize>, TokenKind)>,
    pub lines: usize,
}

pub(crate) fn prepare(code: &str, spans: &[CodeSpan]) -> Prepared {
    let kept = code.strip_suffix('\n').unwrap_or(code);
    let kept = kept.strip_suffix('\r').unwrap_or(kept);
    let mut text = String::with_capacity(kept.len());
    let mut moved = vec![0usize; code.len() + 1];
    let mut column = 0;
    for (at, c) in kept.char_indices() {
        moved[at..at + c.len_utf8()].fill(text.len());
        match c {
            '\t' => {
                let width = TAB_WIDTH - column % TAB_WIDTH;
                text.push_str(&" ".repeat(width));
                column += width;
            }
            '\r' if kept[at + 1..].starts_with('\n') => {}
            '\n' => {
                text.push('\n');
                column = 0;
            }
            c => {
                text.push(c);
                column += 1;
            }
        }
    }
    moved[kept.len()..].fill(text.len());

    let mut runs: Vec<(Range<usize>, TokenKind)> = spans
        .iter()
        .filter(|span| {
            span.range.start < span.range.end
                && span.range.end <= code.len()
                && code.is_char_boundary(span.range.start)
                && code.is_char_boundary(span.range.end)
        })
        .map(|span| (moved[span.range.start]..moved[span.range.end], span.kind))
        .filter(|(range, _)| range.start < range.end)
        .collect();
    runs.sort_by_key(|(range, _)| range.start);
    let lines = text.split('\n').count();
    Prepared { text, runs, lines }
}

fn code_text(inherited: TextStyle) -> TextStyle {
    shared::control_text(inherited, MONO_RATIO)
        .with_font_family(FontFamily::Monospace)
        .with_text_wrap(TextWrap::NoWrap)
}

/// Read-only source with token colours, line numbers, marked lines and a copy button.
pub fn code_view(
    props: CodeViewProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let CodeViewProps {
        code,
        spans,
        highlighted,
        first_line,
        line_numbers,
        copyable,
        on_copy,
        label,
        max_height,
        token_style,
        color,
    } = props;
    let first_line = first_line.max(1);
    let prepared = {
        let code = code.clone();
        memo(move || prepare(&code.get(), &spans.get()))
    };

    let styler: TokenStyler = token_style.unwrap_or_else(|| Rc::new(theme_token_style));
    let source = Text::spanned_declaring(
        move || prepared.with(|prepared| prepared.text.clone()),
        move || {
            prepared.with(|prepared| {
                prepared
                    .runs
                    .iter()
                    .filter(|(_, kind)| *kind != TokenKind::Plain)
                    .map(|(range, kind)| {
                        Span::new(range.start as u32..range.end as u32, styler(*kind))
                    })
                    .collect()
            })
        },
        LayoutStyle::new().flex_shrink(0.0),
        code_text,
    )?;
    let source_node = source.layout_node();

    let marks = Canvas::declaring(
        LayoutStyle::new().absolute_fill(),
        move |rect, inherited| {
            let line = line_box(&code_text(inherited));
            let wash = shared::resolve(&color, shared::accent).with_alpha(MARK_ALPHA);
            let lines = prepared.with(|prepared| prepared.lines) as u32;
            let bands = highlighted
                .get()
                .into_iter()
                .filter(|number| (first_line..first_line + lines).contains(number))
                .map(|number| {
                    let top = pad() + (number - first_line) as f32 * line;
                    RenderNode::rect(
                        Rect::new(0.0, top, rect.width, line),
                        RectStyle::default().with_fill(wash),
                    )
                });
            RenderNode::group(bands)
        },
    )?
    .a11y_hidden();

    let mut row: Vec<Box<dyn LayoutItem>> = vec![box_item(marks)];
    if line_numbers {
        let gutter = LineGutter::starting_at(
            move || first_line as usize,
            move || prepared.with(|prepared| prepared.lines),
            LayoutStyle::new().flex_shrink(0.0),
            move || shared::quiet(code_text(inherited_text_style(source_node)), 1.0),
        )?
        .a11y_hidden();
        row.push(box_item(gutter));
    }
    row.push(box_item(source));
    let content_box = || {
        LayoutStyle::new()
            .flex_row()
            .padding_all(pad())
            .gap(pad() * 1.5)
    };
    let content = Container::new(content_box(), row)?.styled_by(content_box);
    let content_rect = track_layout(content.layout_node()).expect("content node is registered");
    let scroll = LayoutScrollArea::new(LayoutStyle::new().flex_grow(1.0), box_item(content))?;
    let viewport_box = move || {
        let height = content_rect.get().height;
        let height = if max_height > 0.0 {
            height.min(max_height)
        } else {
            height
        };
        LayoutStyle::new().flex_column().height(height)
    };
    let viewport = Container::new(viewport_box(), vec![box_item(scroll)])?.styled_by(viewport_box);

    let mut layers: Vec<Box<dyn LayoutItem>> = vec![box_item(viewport)];
    if copyable {
        layers.push(copy_button(code, on_copy)?);
    }
    let frame = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_r| {
            RectStyle::default()
                .with_fill(shared::surface_alt())
                .with_border(Border::uniform(shared::border(), 1.0))
                .with_radius(BorderRadius::all(shared::radius_md()))
        },
        layers,
    )?
    .role(Role::Group);
    Ok(match label {
        Some(label) => box_item(frame.a11y_label(move || label.get())),
        None => box_item(frame),
    })
}

fn copy_button(
    code: Reactive<String>,
    on_copy: Option<CopyHandler>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let copied = signal(false);
    let reset: Rc<RefCell<Option<Timer>>> = Rc::default();
    let press = Rc::new(move || {
        let text = code.get();
        telar::set_clipboard_text(&text);
        if let Some(on_copy) = &on_copy {
            on_copy(&text);
        }
        copied.set(true);
        *reset.borrow_mut() = Some(run_after(COPIED_FOR, move || copied.set(false)));
    });
    let button = icon_button(
        IconButtonProps::props()
            .icon(telar_icons::icon!("lucide:copy"))
            .label(Reactive::of(move || {
                strings::text(if copied.get() {
                    strings::COPIED
                } else {
                    strings::COPY
                })
            }))
            .on_press(press)
            .build(),
        Children::default(),
    )?;
    let corner = Container::new(
        LayoutStyle::new()
            .absolute()
            .inset_top(pad() * 0.5)
            .inset_end(pad() * 0.5),
        vec![button],
    )?;
    Ok(box_item(corner))
}

#[cfg(test)]
#[path = "code_view_test.rs"]
mod tests;
