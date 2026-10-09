//! [`kbd`]: a key cap per key of a shortcut, for hints beside a command.

use telar::{
    Accessible, AlignItems, Border, BorderRadius, Children, JustifyContent, LayoutError,
    LayoutItem, LayoutStyle, Props, Reactive, ReactiveList, RectStyle, ShapeStyle, StyledContainer,
    Text, TextWrap, box_item,
};

use crate::shared;

const CAP_RATIO: f32 = 0.8;

/// The keys of a shortcut, each on its own cap.
#[derive(Props)]
pub struct KbdProps {
    /// The chord, keys joined by `+`: `"Ctrl+K"`, `"Shift+Alt+P"`. A key that is itself `+` is written last, `"Ctrl++"`. `Mod` is the platform's command key: `⌘` on macOS, `Ctrl` elsewhere.
    #[props(into, default)]
    pub chord: Reactive<String>,
}

/// One cap per key of `chord`.
pub fn kbd(props: KbdProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let KbdProps { chord } = props;
    let spoken = chord.clone();
    let caps = ReactiveList::with_style(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(shared::spacing() * 0.4),
        move || {
            split_chord(&chord.get())
                .into_iter()
                .enumerate()
                .collect::<Vec<_>>()
        },
        |(position, key): &(usize, String)| (*position, key.clone()),
        |(_, key)| cap(display_name(&key, cfg!(target_os = "macos"))),
    )?
    .a11y_hidden();
    let row = StyledContainer::new(
        LayoutStyle::new().flex_row(),
        |_| RectStyle::default(),
        vec![box_item(caps)],
    )?
    .a11y_label(move || spoken_chord(&spoken.get()));
    Ok(box_item(row))
}

fn cap(label: String) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(
        move || label.clone(),
        LayoutStyle::new(),
        |t| shared::quiet(t, CAP_RATIO).with_text_wrap(TextWrap::NoWrap),
    )?;
    let style = || {
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .min_width(shared::spacing() * 2.5)
            .padding_horizontal(shared::spacing() * 0.6)
            .padding_vertical(shared::spacing() * 0.15)
    };
    let cap = StyledContainer::new(
        style(),
        |_| {
            RectStyle::default()
                .with_fill(shared::surface_alt())
                .with_border(Border::uniform(shared::border(), 1.0))
                .with_radius(BorderRadius::all(shared::radius_sm()))
        },
        vec![box_item(text)],
    )?
    .styled_by(style);
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
