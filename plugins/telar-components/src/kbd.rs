//! [`kbd`]: a key cap per key of a shortcut, for hints beside a command.

use telar::{
    Accessible, AlignItems, Border, BorderRadius, Chord, Container, JustifyContent, LayoutError,
    LayoutItem, LayoutStyle, NodeId, Props, Reactive, ReactiveList, RectStyle, ShapeStyle,
    StyledContainer, SurfaceAmendment, Text, TextWrap, amend_surface, box_item, children,
    keyboard_convention, style_follows,
};

use crate::shared::{self, shown_chord};

/// The keys of a shortcut, each on its own cap.
#[derive(Props)]
pub struct KbdProps {
    /// The chord, written portably as [`Chord`] reads it: `"Mod+K"`, `"Alt+Shift+P"`, `"Ctrl++"`. Its caps are the keys the platform names, `⌘` `K` on a Mac and `Ctrl` `K` elsewhere, and text that names no chord is one cap, as written.
    #[props(into, default)]
    pub chord: Reactive<String>,
    /// Amends the paint of every cap — the chord's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`KbdTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// The key caps' own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when a cap paints, so a theme that moves the role still moves every cap it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KbdTokens {
    /// Corner radius of a cap. `None` is the theme's `radius_item`: a cap sits inside a row or a tooltip.
    pub radius: Option<f32>,
    /// Width of a cap's frame. `None` is the theme's `border_width`.
    pub border_width: Option<f32>,
}

impl KbdTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_item)
    }

    pub fn border_width(&self) -> f32 {
        self.border_width.unwrap_or_else(shared::border_width)
    }
}

/// One cap per key of `chord`.
pub fn kbd(props: KbdProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let KbdProps { chord, style } = props;
    let spoken = chord.clone();
    let caps = ReactiveList::with_style(
        caps_row(),
        move || {
            shown_keys(&chord.get())
                .into_iter()
                .enumerate()
                .collect::<Vec<_>>()
        },
        |(position, key): &(usize, String)| (*position, key.clone()),
        move |(_, key)| cap(key, style.clone()),
    )?
    .a11y_hidden();
    style_follows(caps.layout_node(), caps_row);
    let row = Container::new(LayoutStyle::row(), children![caps])?
        .a11y_label(move || shown_chord(&spoken.get()));
    Ok(box_item(row))
}

fn caps_row() -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .gap(shared::gap_xs())
}

/// A cap around its key, never narrower than it is tall, so a one-letter key is a square.
fn cap_box(text: NodeId) -> LayoutStyle {
    let pad_y = shared::gap_xs();
    let line = telar::single_line_box(shared::control_text_size(text, shared::CAPTION_RATIO));
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .min_width(line + pad_y * 2.0)
        .padding_horizontal(shared::gap_sm())
        .padding_vertical(pad_y)
}

fn cap(label: String, style: Option<SurfaceAmendment>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(
        move || label.clone(),
        LayoutStyle::new(),
        |t| shared::quiet(t, shared::CAPTION_RATIO).with_text_wrap(TextWrap::NoWrap),
    )?;
    let text_node = text.layout_node();
    let cap = StyledContainer::new(
        cap_box(text_node),
        move |_| {
            let tokens = KbdTokens::current();
            let rect = RectStyle::default()
                .with_fill(shared::surface_alt())
                .with_border(Border::uniform(shared::border(), tokens.border_width()))
                .with_radius(BorderRadius::all(tokens.radius()));
            amend_surface(rect, &style)
        },
        children![text],
    )?
    .styled_by(move || cap_box(text_node));
    Ok(box_item(cap))
}

/// The keys of `chord` the way the platform names them, one per cap. Text that names no [`Chord`] is one cap, as written.
fn shown_keys(chord: &str) -> Vec<String> {
    match Chord::parse(chord) {
        Ok(parsed) => parsed.keys(keyboard_convention()),
        Err(_) if chord.trim().is_empty() => Vec::new(),
        Err(_) => vec![chord.trim().to_string()],
    }
}

#[cfg(test)]
#[path = "kbd_test.rs"]
mod tests;
