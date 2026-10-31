//! [`tabs`]: a row of pills selecting one index, and [`tab_list`], the same row selecting any value.

use std::cell::RefCell;
use std::rc::Rc;

use telar::focus::{self, FocusId};
use telar::{
    Accessible, AlignItems, BorderRadius, Children, Color, Container, JustifyContent, Key, KeyNav,
    KeyNavMove, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, NodeId, Props, Reactive,
    Rect, RectStyle, RwSignal, ShapeStyle, StyledContainer, SurfaceAmendment, Text, TextStyle,
    amend_surface, box_item, current_direction, effect, focus::Role, key_nav_apply, signal,
    track_layout,
};

#[cfg(feature = "workbench")]
use crate::icon_button::{IconSource, picture};
use crate::shared;

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
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_horizontal(TabsTokens::current().pad_x())
        .gap(shared::gap_sm())
        .min_height(shared::control_height())
}
#[cfg(feature = "workbench")]
fn icon_tab_box() -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_all(shared::icon_pad())
        .min_width(shared::control_height())
        .min_height(shared::control_height())
}
fn bar() -> LayoutStyle {
    LayoutStyle::row().gap(shared::gap_sm())
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
#[derive(Clone)]
pub struct Tab<T> {
    value: T,
    label: Reactive<String>,
    trailing: Children,
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
            trailing: Children::default(),
            #[cfg(feature = "workbench")]
            icon: None,
            #[cfg(feature = "workbench")]
            shortcut: None,
        }
    }

    /// Draws `children` after the tab's name. They are not part of the name a reader hears.
    pub fn trailing(mut self, children: Children) -> Self {
        self.trailing = children;
        self
    }

    #[cfg(feature = "workbench")]
    /// Draws `icon` in place of the tab's name, which a tooltip then shows and a reader still hears: a row too tight for words.
    pub fn icon(mut self, icon: impl Into<IconSource>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    #[cfg(feature = "workbench")]
    /// The key binding that selects this tab from anywhere, written portably (`"Alt+1"`) and shown in a tooltip over it as the platform writes it.
    pub fn shortcut(mut self, shortcut: impl Into<Reactive<String>>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
}

/// A row of pills selecting one index.
pub fn tabs(props: TabsProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
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

    let nodes: Rc<[NodeId]> = tab_items.iter().map(|tab| tab.layout_node()).collect();
    let row = Container::new(bar(), tab_items)?
        .styled_by(bar)
        .role(Role::TabList);
    let list = match label {
        Some(label) => box_item(row.a11y_label(move || label.get())),
        None => box_item(row),
    };
    let stepping = Stepping {
        selected,
        values,
        nodes,
    };
    scrolling(list, stepping)
}

/// `list` in a strip that scrolls it sideways, by the wheel as well, once it is wider than the room it is given, with the tab chosen kept in its middle and an arrow at either end that chooses the tab before or after it. The arrows are no Tab stops and say nothing to a reader: the keys that walk the tabs already do what they do.
fn scrolling<T>(
    list: Box<dyn LayoutItem>,
    stepping: Stepping<T>,
) -> Result<Box<dyn LayoutItem>, LayoutError>
where
    T: Copy + PartialEq + 'static,
{
    let list_rect = track_layout(list.layout_node()).expect("a tab list is registered");
    let mut held = None;
    let scroll = LayoutScrollArea::new_with(
        LayoutStyle::row().flex_shrink(1.0).min_width(0.0),
        |viewport| {
            held = Some(viewport);
            Ok(list)
        },
    )?
    .wheel_sideways();
    let viewport = held.expect("a scroll area builds its content before it returns");
    let centring = stepping.clone();
    // Follows where the chosen tab stands and how wide the view is, never the offset, so scrolling the strip by hand is left alone until another tab is chosen.
    effect(move || {
        let Some(tab) = centring.chosen_node().and_then(track_layout) else {
            return;
        };
        let (tab, view) = (tab.get(), viewport.rect().get());
        let furthest = (list_rect.peek().width - view.width).max(0.0);
        let middle = tab.x + tab.width / 2.0 - view.width / 2.0;
        viewport.scroll_to(middle.clamp(0.0, furthest), 0.0);
    });
    let laid = signal(None::<RwSignal<Rect>>);
    let overflows = move || {
        laid.get()
            .is_some_and(|strip| overflows(list_rect.get().width, strip.get().width))
    };
    let strip_box = || {
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(1.0)
            .min_width(0.0)
    };
    let (back, forward) = (stepping.clone(), stepping);
    let strip = Container::new(
        strip_box(),
        vec![
            shared::shown_while(overflows, LayoutStyle::row(), move || {
                arrow(back.clone(), false)
            })?,
            box_item(scroll),
            shared::shown_while(overflows, LayoutStyle::row(), move || {
                arrow(forward.clone(), true)
            })?,
        ],
    )?
    .styled_by(strip_box);
    laid.set(track_layout(strip.layout_node()));
    Ok(box_item(strip))
}

/// Whether a list `list` wide overflows a strip `strip` wide: by more than the pixel snapping can open between the two, since the strip's edges are snapped to whole pixels and the list's width is not.
///
/// Against the whole strip rather than the part the arrows leave. A strip with room to spare is as wide as it asks, arrows and all, so were a list a fraction wider than its snapped strip read as overflowing, the arrows would widen the strip, take themselves away, and come back the next frame.
fn overflows(list: f32, strip: f32) -> bool {
    list > strip + 1.0
}

/// What the arrows move: the selection, along the tabs in their order.
struct Stepping<T: 'static> {
    selected: RwSignal<T>,
    values: Rc<[T]>,
    nodes: Rc<[NodeId]>,
}

impl<T: 'static> Clone for Stepping<T> {
    fn clone(&self) -> Self {
        Self {
            selected: self.selected,
            values: Rc::clone(&self.values),
            nodes: Rc::clone(&self.nodes),
        }
    }
}

impl<T: Copy + PartialEq + 'static> Stepping<T> {
    fn chosen(&self) -> Option<usize> {
        let at = self.selected.get();
        self.values.iter().position(|value| *value == at)
    }

    fn chosen_node(&self) -> Option<NodeId> {
        self.chosen()
            .and_then(|index| self.nodes.get(index).copied())
    }

    /// The tab an arrow toward the end of the line, or toward its start, would choose: `None` past the last or the first.
    fn next(&self, toward_end: bool) -> Option<usize> {
        let at = self.chosen()?;
        match toward_end {
            true => (at + 1 < self.values.len()).then_some(at + 1),
            false => at.checked_sub(1),
        }
    }

    fn step(&self, toward_end: bool) {
        if let Some(next) = self.next(toward_end) {
            self.selected.set(self.values[next]);
        }
    }
}

fn arrow<T: Copy + PartialEq + 'static>(
    stepping: Stepping<T>,
    toward_end: bool,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let spent = {
        let stepping = stepping.clone();
        move || stepping.next(toward_end).is_none()
    };
    let shape = move || match toward_end {
        true => shared::Mark::ChevronForward,
        false => shared::Mark::ChevronBack,
    };
    let faded = spent.clone();
    let face = shared::mark(shape, move |text| match faded() {
        true => text.color.faded(shared::QUIET_ALPHA * shared::QUIET_ALPHA),
        false => text.color.faded(shared::QUIET_ALPHA),
    })?;
    let wash = |fill: fn() -> Color| {
        let spent = spent.clone();
        move |_r| {
            let rect =
                RectStyle::default().with_radius(BorderRadius::all(TabsTokens::current().radius()));
            match spent() {
                true => rect,
                false => rect.with_fill(fill()),
            }
        }
    };
    let arrow_box = || {
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .flex_shrink(0.0)
            .width(shared::icon_size() + shared::icon_pad() * 2.0)
            .min_height(shared::control_height())
    };
    let pressed = spent.clone();
    let arrow = StyledContainer::new(arrow_box(), |_r| RectStyle::default(), vec![face])?
        .styled_by(arrow_box)
        .hover_style(wash(shared::hover_wash))
        .active_style(wash(shared::pressed_wash))
        .on_press(move || {
            if !pressed() {
                stepping.step(toward_end);
            }
        })
        .a11y_hidden();
    Ok(box_item(arrow))
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
    #[cfg(feature = "workbench")]
    let hint = (entry.icon.is_some() || entry.shortcut.is_some())
        .then(|| (entry.label.clone(), entry.shortcut.clone()));
    let button = TabButton {
        selected,
        entry,
        index,
        paint: paint.clone(),
        values: Rc::clone(values),
        focus_ids: Rc::clone(focus_ids),
    };
    #[cfg(feature = "workbench")]
    if let Some((text, shortcut)) = hint {
        return shared::hinted(move || button.build(), text, shortcut.unwrap_or_default());
    }
    button.build()
}

/// Everything one tab is built from, so a tooltip wrapping it can build it wherever and however often it places it.
#[derive(Clone)]
struct TabButton<T: 'static> {
    selected: RwSignal<T>,
    entry: Tab<T>,
    index: usize,
    paint: Paint,
    values: Rc<[T]>,
    focus_ids: Rc<RefCell<Vec<FocusId>>>,
}

impl<T> TabButton<T>
where
    T: Copy + PartialEq + 'static,
{
    fn build(&self) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let TabButton {
            selected,
            entry,
            index,
            paint,
            values,
            focus_ids,
        } = self.clone();
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
        let (face, shape) = match icon {
            Some(icon) => (pictured(icon, selected, value)?, icon_tab_box as fn() -> _),
            None => (named(label, selected, value)?, tab_box as fn() -> _),
        };
        #[cfg(not(feature = "workbench"))]
        let (face, shape) = (named(label, selected, value)?, tab_box as fn() -> _);
        let mut content = vec![face];
        content.extend(trailing.build()?);

        let (rest, hover, press) = (paint.clone(), paint.clone(), paint);
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
        #[cfg(feature = "workbench")]
        let tab = match shortcut {
            Some(chord) => tab.keyboard_shortcut(move || chord.get()),
            None => tab,
        };
        if let Some(id) = tab.focus_id() {
            let mut ids = focus_ids.borrow_mut();
            // A rebuilt tab takes its own place rather than a new one, so the arrows still land on the tab at each index.
            match ids.get_mut(index) {
                Some(placed) => *placed = id,
                None => ids.push(id),
            }
        }
        Ok(box_item(tab.on_focused_key(move |key: &Key| {
            let Some(next) = step(index, values.len(), key) else {
                return false;
            };
            selected.set(values[next]);
            if let Some(id) = focus_ids.borrow().get(next) {
                focus::request(*id);
            }
            true
        })))
    }
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
