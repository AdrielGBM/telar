//! [`tabs`]: a row of pills selecting one index, and [`tab_list`], the same row selecting any value.

use std::cell::RefCell;
use std::rc::Rc;

use telar::focus::{self, FocusId};
use telar::{
    Accessible, AlignItems, BorderRadius, Children, Color, Container, JustifyContent, Key, KeyNav,
    KeyNavMove, LayoutError, LayoutItem, LayoutStyle, Props, Reactive, RectStyle, RwSignal,
    ShapeStyle, StyledContainer, SurfaceAmendment, Text, TextStyle, amend_surface, box_item,
    current_direction, focus::Role, key_nav_apply, signal,
};

#[cfg(feature = "workbench")]
use crate::icon_button::{IconSource, picture};
use crate::shared;
#[cfg(feature = "workbench")]
use crate::{TooltipProps, tooltip};

/// The tabs' own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when a tab paints and lays out, so a theme that moves the role still moves every tab it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TabsTokens {
    /// Corner radius of each tab. `None` is the theme's `radius_control`.
    pub radius: Option<f32>,
    /// Space between a tab's edge and its name at the regular control size, scaled with the control size in force. `None` is [`metrics::control_pad_x`](crate::metrics::control_pad_x).
    pub pad_x: Option<f32>,
}

impl TabsTokens {
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
}

fn tab_box() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_horizontal(TabsTokens::current().pad_x())
        .min_height(shared::control_height())
}
#[cfg(feature = "workbench")]
fn icon_tab_box() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_all(shared::icon_pad())
        .min_width(shared::control_height())
        .min_height(shared::control_height())
}
fn bar() -> LayoutStyle {
    LayoutStyle::new().flex_row().gap(shared::gap_sm())
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
    /// Amends the paint of every tab — the bar's **principal surface** — selected or not, at rest, hovered and pressed. See [`telar::SurfaceStyle`] for when a theme token or [`TabsTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// One tab of a [`tab_list`]: the value it selects, its name, and optionally an item drawn after the name, such as a count, or an icon drawn in its place.
pub struct Tab<T> {
    value: T,
    label: Reactive<String>,
    trailing: Option<Box<dyn LayoutItem>>,
    #[cfg(feature = "workbench")]
    icon: Option<IconSource>,
    #[cfg(feature = "workbench")]
    shortcut: Option<Reactive<String>>,
}

impl<T> Tab<T> {
    /// A tab selecting `value`, named `label`; a reactive label follows the language or whatever else it reads.
    pub fn new(value: T, label: impl Into<Reactive<String>>) -> Self {
        Self {
            value,
            label: label.into(),
            trailing: None,
            #[cfg(feature = "workbench")]
            icon: None,
            #[cfg(feature = "workbench")]
            shortcut: None,
        }
    }

    /// Draws `item` after the tab's name. It is not part of the name a reader hears.
    pub fn trailing(mut self, item: Box<dyn LayoutItem>) -> Self {
        self.trailing = Some(item);
        self
    }

    #[cfg(feature = "workbench")]
    /// Draws `icon` in place of the tab's name, which a tooltip then shows and a reader still hears: a row too tight for words.
    pub fn icon(mut self, icon: impl Into<IconSource>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    #[cfg(feature = "workbench")]
    /// The key binding that selects this tab from anywhere, shown in a tooltip over it: `"Alt+1"`.
    pub fn shortcut(mut self, shortcut: impl Into<Reactive<String>>) -> Self {
        self.shortcut = Some(shortcut.into());
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
        style,
    } = props;
    // Uncontrolled: own the index so the bar still tracks the active tab when the caller binds no signal.
    let selected = selected.unwrap_or_else(|| signal(0u32));
    let entries = items
        .into_iter()
        .zip(0u32..)
        .map(|(name, index)| Tab::new(index, name))
        .collect();
    tab_row(selected, entries, color, label, style)
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
    tab_row(selected, entries, color, label, None)
}

fn tab_row<T>(
    selected: RwSignal<T>,
    entries: Vec<Tab<T>>,
    color: Reactive<Color>,
    label: Option<Reactive<String>>,
    style: Option<SurfaceAmendment>,
) -> Result<Box<dyn LayoutItem>, LayoutError>
where
    T: Copy + PartialEq + 'static,
{
    let values: Rc<[T]> = entries.iter().map(|entry| entry.value).collect();
    let focus_ids: Rc<RefCell<Vec<FocusId>>> = Rc::default();
    let paint = Paint { color, style };
    let mut tab_items: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(entries.len());
    for (index, entry) in entries.into_iter().enumerate() {
        tab_items.push(tab(selected, entry, index, &paint, &values, &focus_ids)?);
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
    paint: &Paint,
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
        #[cfg(feature = "workbench")]
        icon,
        #[cfg(feature = "workbench")]
        shortcut,
    } = entry;
    let name = label.clone();
    #[cfg(feature = "workbench")]
    let hint = (icon.is_some() || shortcut.is_some()).then(|| (label.clone(), shortcut));
    #[cfg(feature = "workbench")]
    let (face, shape) = match icon {
        Some(icon) => (pictured(icon, selected, value)?, icon_tab_box as fn() -> _),
        None => (named(label, selected, value)?, tab_box as fn() -> _),
    };
    #[cfg(not(feature = "workbench"))]
    let (face, shape) = (named(label, selected, value)?, tab_box as fn() -> _);
    let mut content = vec![face];
    content.extend(trailing);

    let (rest, hover, press) = (paint.clone(), paint.clone(), paint.clone());
    let tab = StyledContainer::new(
        shape(),
        move |_r| rest.surface(selected.get() == value, State::Rest),
        content,
    )?
    .styled_by(shape)
    .hover_style(move |_r| hover.surface(selected.get() == value, State::Hovered))
    .active_style(move |_r| press.surface(selected.get() == value, State::Pressed))
    .control(Role::Tab)
    .toggled(move || selected.get() == value)
    .on_press(move || selected.set(value))
    .a11y_label(move || name.get());
    if let Some(id) = tab.focus_id() {
        focus_ids.borrow_mut().push(id);
    }
    let values = Rc::clone(values);
    let focus_ids = Rc::clone(focus_ids);
    let tab = box_item(tab.on_focused_key(move |key: &Key| {
        let Some(next) = step(index, values.len(), key) else {
            return false;
        };
        selected.set(values[next]);
        if let Some(id) = focus_ids.borrow().get(next) {
            focus::request(*id);
        }
        true
    }));
    #[cfg(feature = "workbench")]
    if let Some((text, shortcut)) = hint {
        return hinted(tab, text, shortcut);
    }
    Ok(tab)
}

fn named<T>(
    label: Reactive<String>,
    selected: RwSignal<T>,
    value: T,
) -> Result<Box<dyn LayoutItem>, LayoutError>
where
    T: Copy + PartialEq + 'static,
{
    Ok(box_item(Text::declaring(
        move || label.get(),
        LayoutStyle::new(),
        move |t| tab_text(t, selected.get() == value),
    )?))
}

/// The icon a tab is drawn as: on the accent pill while selected, and in the ink around it otherwise.
#[cfg(feature = "workbench")]
fn pictured<T>(
    icon: IconSource,
    selected: RwSignal<T>,
    value: T,
) -> Result<Box<dyn LayoutItem>, LayoutError>
where
    T: Copy + PartialEq + 'static,
{
    picture(
        icon,
        Reactive::of(move || {
            if selected.get() == value {
                shared::on_accent()
            } else {
                Color::TRANSPARENT
            }
        }),
    )
}

/// `tab` under a tooltip naming it, and the binding that selects it when there is one.
#[cfg(feature = "workbench")]
fn hinted(
    tab: Box<dyn LayoutItem>,
    text: Reactive<String>,
    shortcut: Option<Reactive<String>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut slots = telar::Slots::new();
    slots.push(None, tab);
    tooltip(
        TooltipProps::props()
            .text(text)
            .shortcut(shortcut.unwrap_or_default())
            .build(),
        Children::from(slots),
    )
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

/// Which of its paints a tab is showing.
#[derive(Clone, Copy)]
enum State {
    Rest,
    Hovered,
    Pressed,
}

/// What every tab of one bar paints with: the accent the selected tab fills with, and the caller's amendment.
#[derive(Clone)]
struct Paint {
    color: Reactive<Color>,
    style: Option<SurfaceAmendment>,
}

impl Paint {
    /// A tab's paint in `state`: the selected tab fills with the accent and darkens under the pointer like any filled control; the others take the theme's washes.
    fn surface(&self, active: bool, state: State) -> RectStyle {
        let rect =
            RectStyle::default().with_radius(BorderRadius::all(TabsTokens::current().radius()));
        let rect = if active {
            let accent = shared::resolve(&self.color, shared::accent);
            rect.with_fill(match state {
                State::Rest => accent,
                State::Hovered => shared::hovered_fill(accent),
                State::Pressed => shared::pressed_fill(accent),
            })
        } else {
            match state {
                State::Rest => rect,
                State::Hovered => rect.with_fill(shared::hover_wash()),
                State::Pressed => rect.with_fill(shared::pressed_wash()),
            }
        };
        amend_surface(rect, &self.style)
    }
}

/// A tab's label: legible on the accent pill when it is the selected one, and otherwise a quieter shade of whatever the bar around it is written in.
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
