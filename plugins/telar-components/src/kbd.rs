//! [`kbd`]: a key cap per key of a shortcut, for hints beside a command.

use telar::{
    Accessible, AlignItems, Border, BorderRadius, Children, JustifyContent, LayoutError,
    LayoutItem, LayoutStyle, NodeId, Props, Reactive, ReactiveList, RectStyle, ShapeStyle,
    StyledContainer, SurfaceAmendment, Text, TextWrap, amend_surface, box_item, style_follows,
};

use crate::shared;

/// The keys of a shortcut, each on its own cap.
#[derive(Props)]
pub struct KbdProps {
    /// The chord, keys joined by `+`: `"Ctrl+K"`, `"Shift+Alt+P"`. A key that is itself `+` is written last, `"Ctrl++"`. `Mod` is the platform's command key: `⌘` on macOS, `Ctrl` elsewhere.
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
pub fn kbd(props: KbdProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let KbdProps { chord, style } = props;
    let spoken = chord.clone();
    let caps = ReactiveList::with_style(
        caps_row(),
        move || {
            split_chord(&chord.get())
                .into_iter()
                .enumerate()
                .collect::<Vec<_>>()
        },
        |(position, key): &(usize, String)| (*position, key.clone()),
        move |(_, key)| cap(display_name(&key, cfg!(target_os = "macos")), style.clone()),
    )?
    .a11y_hidden();
    style_follows(caps.layout_node(), caps_row);
    let row = StyledContainer::new(
        LayoutStyle::new().flex_row(),
        |_| RectStyle::default(),
        vec![box_item(caps)],
    )?
    .a11y_label(move || spoken_chord(&spoken.get()));
    Ok(box_item(row))
}

fn caps_row() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .gap(shared::gap_xs())
}

/// A cap around its key, never narrower than it is tall, so a one-letter key is a square.
fn cap_box(text: NodeId) -> LayoutStyle {
    let pad_y = shared::gap_xs();
    let line = telar::single_line_box(shared::control_text_size(text, shared::CAPTION_RATIO));
    LayoutStyle::new()
        .flex_row()
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
        vec![box_item(text)],
    )?
    .styled_by(move || cap_box(text_node));
    Ok(box_item(cap))
}

/// What a reader is told a chord is: its keys by the names their caps show, joined as they are pressed.
pub(crate) fn spoken_chord(chord: &str) -> String {
    split_chord(chord)
        .iter()
        .map(|key| display_name(key, cfg!(target_os = "macos")))
        .collect::<Vec<_>>()
        .join("+")
}

pub(crate) fn split_chord(chord: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut current = String::new();
    for c in chord.chars() {
        if c == '+' && !current.is_empty() {
            keys.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    keys.push(current);
    keys.into_iter()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .collect()
}

pub(crate) fn display_name(key: &str, mac: bool) -> String {
    let named = match (key.to_ascii_lowercase().as_str(), mac) {
        ("mod", true) | ("cmd" | "command" | "meta", true) => "⌘",
        ("mod", false) => "Ctrl",
        ("ctrl" | "control", true) => "⌃",
        ("alt" | "option", true) => "⌥",
        ("shift", true) => "⇧",
        _ => return key.to_string(),
    };
    named.to_string()
}

#[cfg(test)]
#[path = "kbd_test.rs"]
mod tests;
