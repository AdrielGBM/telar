//! [`tabs`]: a row of pills selecting one index, and [`tab_list`], the same row selecting any value.

use std::cell::RefCell;
use std::rc::Rc;

use telar::focus::{self, FocusId};
use telar::{
    Accessible, AlignItems, BorderRadius, Children, Color, Container, JustifyContent, Key, KeyNav,
    KeyNavMove, LayoutError, LayoutItem, LayoutStyle, Props, Reactive, RectStyle, RwSignal,
    ShapeStyle, StyledContainer, Text, TextStyle, box_item, current_direction, focus::Role,
    key_nav_apply, signal,
};

use crate::shared;

fn pad_x() -> f32 {
    shared::spacing() * 2.0
}
fn pad_y() -> f32 {
    shared::spacing()
}
fn gap() -> f32 {
    shared::spacing() * 0.5
}
fn radius() -> f32 {
    shared::radius() * 1.5
}
fn tab_box() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_horizontal(pad_x())
        .padding_vertical(pad_y())
}
fn bar() -> LayoutStyle {
    LayoutStyle::new().flex_row().gap(gap())
}

/// A horizontal tab bar: one button per label, driving a `selected` index. Renders only the row of tab buttons — the matching content panel is the caller's responsibility (typically the DSL's reactive `if selected == i`, mirroring the sandbox's own nav-button/section-switch split). Modelled on `select.rs`'s items/selected handling, but rendered inline (a row) rather than as an anchored overlay. High-level sugar over the primitives; lives in `telar-components`, not the kernel.
#[derive(Props)]
pub struct TabsProps {
    /// The tab labels, rendered in order.
    #[props(default)]
    pub items: Vec<&'static str>,
    /// Bound active index. `None` (the default) is uncontrolled — the widget owns its own `signal(0)`.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<u32>>,
    /// Accent (active tab fill). `Color::TRANSPARENT` (the default) means "unset": falls back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// What a reader calls the set of tabs: "Settings sections". `None` (the default) leaves the list unnamed.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
}

/// One tab of a [`tab_list`]: the value it selects, its name, and optionally an item drawn after the name, such as a count.
pub struct Tab<T> {
    value: T,
    label: Reactive<String>,
    trailing: Option<Box<dyn LayoutItem>>,
}

impl<T> Tab<T> {
    /// A tab selecting `value`, named `label`; a reactive label follows the language or whatever else it reads.
    pub fn new(value: T, label: impl Into<Reactive<String>>) -> Self {
        Self {
            value,
            label: label.into(),
            trailing: None,
        }
    }

    /// Draws `item` after the tab's name. It is not part of the name a reader hears.
    pub fn trailing(mut self, item: Box<dyn LayoutItem>) -> Self {
        self.trailing = Some(item);
        self
    }
}

/// A row of pills selecting one index.
pub fn tabs(props: TabsProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let TabsProps {
        items,
        selected,
        color,
        label,
    } = props;
    // Uncontrolled: own the index so the bar still tracks the active tab when the caller binds no signal.
    let selected = selected.unwrap_or_else(|| signal(0u32));
    let entries = items
        .into_iter()
        .zip(0u32..)
        .map(|(name, index)| Tab::new(index, name))
        .collect();
    tab_list(selected, entries, color, label)
}

/// The row of pills behind [`tabs`], selecting any `Copy` value instead of an index, so a caller whose selection is an enum binds it directly.
///
/// While one tab holds focus the arrows (following the writing direction), Home and End move the selection and focus along the row, wrapping at either end.
pub fn tab_list<T>(
    selected: RwSignal<T>,
    entries: Vec<Tab<T>>,
    color: Reactive<Color>,
    label: Option<Reactive<String>>,
) -> Result<Box<dyn LayoutItem>, LayoutError>
where
    T: Copy + PartialEq + 'static,
{
    let values: Rc<[T]> = entries.iter().map(|entry| entry.value).collect();
    let focus_ids: Rc<RefCell<Vec<FocusId>>> = Rc::default();
    let mut tab_items: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(entries.len());
    for (index, entry) in entries.into_iter().enumerate() {
        tab_items.push(tab(selected, entry, index, &color, &values, &focus_ids)?);
    }

    let row = Container::new(bar(), tab_items)?
        .styled_by(bar)
        .role(Role::TabList);
    Ok(match label {
        Some(label) => box_item(row.a11y_label(move || label.get())),
        None => box_item(row),
    })
}

fn tab<T>(
    selected: RwSignal<T>,
    entry: Tab<T>,
    index: usize,
    color: &Reactive<Color>,
    values: &Rc<[T]>,
    focus_ids: &Rc<RefCell<Vec<FocusId>>>,
) -> Result<Box<dyn LayoutItem>, LayoutError>
where
    T: Copy + PartialEq + 'static,
{
    let Tab {
        value,
        label,
        trailing,
    } = entry;
    let name = label.clone();
    let name_widget = Text::declaring(
        move || label.get(),
        LayoutStyle::new(),
        move |t| tab_text(t, selected.get() == value),
    )?;
    let mut content = vec![box_item(name_widget)];
    content.extend(trailing);

    let base_color = color.clone();
    let hover_color = color.clone();
    let tab = StyledContainer::new(
        tab_box(),
        move |_r| tab_rect(selected.get() == value, &base_color, false),
        content,
    )?
    .styled_by(tab_box)
    .hover_style(move |_r| tab_rect(selected.get() == value, &hover_color, true))
    .control(Role::Tab)
    .toggled(move || selected.get() == value)
    .on_press(move || selected.set(value))
    .a11y_label(move || name.get());
    if let Some(id) = tab.focus_id() {
        focus_ids.borrow_mut().push(id);
    }
    let values = Rc::clone(values);
    let focus_ids = Rc::clone(focus_ids);
    let tab = tab.on_focused_key(move |key: &Key| {
        let Some(next) = step(index, values.len(), key) else {
            return false;
        };
        selected.set(values[next]);
        if let Some(id) = focus_ids.borrow().get(next) {
            focus::request(*id);
        }
        true
    });
    Ok(box_item(tab))
}

/// The tab `key` moves to from the one at `at` among `count`, or `None` for a key that is not along the row.
fn step(at: usize, count: usize, key: &Key) -> Option<usize> {
    let movement = KeyNav::default()
        .horizontal()
        .reading(current_direction())
        .interpret(key)?;
    match movement {
        KeyNavMove::Next | KeyNavMove::Previous | KeyNavMove::First | KeyNavMove::Last => {
            (count > 0).then(|| key_nav_apply(at, count, movement))
        }
        _ => None,
    }
}

/// The tab pill's paint: the active tab fills with the accent (a touch darker on hover); an inactive tab blends in until hovered, when it lifts to a faint accent wash.
fn tab_rect(active: bool, color: &Reactive<Color>, hovered: bool) -> RectStyle {
    let radius = BorderRadius::all(radius());
    let accent = shared::resolve(color, shared::accent);
    if active {
        let fill = if hovered { accent.darken(0.15) } else { accent };
        return RectStyle::default().with_fill(fill).with_radius(radius);
    }
    if hovered {
        return RectStyle::default()
            .with_fill(accent.with_alpha(0.10))
            .with_radius(radius);
    }
    RectStyle::default().with_radius(radius)
}

/// A tab's label: legible on the accent pill when it is the selected one, and otherwise a quieter shade of whatever the bar around it is written in.
///
/// The inactive tone was a flat grey literal — the same grey in dark mode, and the same grey under a region that had declared its own ink. It was also, to two decimals, what fading the default ink over a white page produces: a light-mode screenshot of a value the cascade already knows how to work out.
fn tab_text(inherited: TextStyle, active: bool) -> TextStyle {
    if active {
        shared::control_text(inherited, 1.0).with_color(shared::on_accent())
    } else {
        shared::quiet(inherited, 1.0)
    }
}

#[cfg(test)]
#[path = "tabs_test.rs"]
mod tests;
