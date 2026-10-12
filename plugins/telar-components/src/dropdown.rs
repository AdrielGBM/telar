//! What a control that drops a panel is made of — its trigger, the overlay shown while it is open, the dismissal, the panel's place under it — shared by `menu` and `select`, and the list box a `select` opens.

use std::rc::Rc;
use telar::{
    AlignItems, Anchoring, Border, BorderRadius, Children, Clip, ClippedItem, Color, ConsumedKeys,
    Container, DismissRegistration, HasPopup, JustifyContent, Key, LayoutError, LayoutItem,
    LayoutStyle, Memo, NamedKey, NodeId, Overlay, Placement, Reactive, ReactiveList, Rect,
    RectStyle, RwSignal, ShapeStyle, StyledContainer, SurfaceStyle, Text, TextWrap, amend_surface,
    box_item, effect,
    focus::{FocusId, Role},
    memo, signal, track_layout,
};

use crate::popup;
use crate::shared;

/// How many ems wide a trigger and the panel it opens are when their tokens name no width: room for a two-word label beside the caret, and a width the panel lines up under.
const WIDTH_EMS: f32 = 13.0;

/// The width a trigger and its panel take: what the tokens of the component they belong to name, else [`WIDTH_EMS`].
pub(crate) fn width(token: Option<f32>) -> f32 {
    shared::width_or_measure(token, WIDTH_EMS)
}

/// The space between a panel and the control it drops from: what the tokens of the component it belongs to name, at the regular control size and scaled with the control size in force, else [`metrics::popup_offset`](crate::metrics::popup_offset).
pub(crate) fn offset(token: Option<f32>) -> f32 {
    token.map_or_else(shared::popup_offset, |offset| {
        offset * telar::control_scale()
    })
}

/// What a trigger is drawn at, resolved from the tokens of the component it belongs to: a `menu`'s or a `select`'s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Geometry {
    pub radius: f32,
    pub pad_x: f32,
    pub border_width: f32,
    pub width: f32,
}

/// What a trigger shows and how it is shaped.
pub(crate) struct Face {
    pub label: Reactive<String>,
    /// The colour a bordered trigger is framed in.
    pub color: Reactive<Color>,
    pub bordered: bool,
    pub caret: bool,
    /// Whether the column the trigger stands in decides its width — a stretched or fitted trigger — rather than [`Geometry::width`].
    pub sized_by_root: bool,
    /// Read wherever the trigger is laid out or painted, so a theme or a control size that changes moves it.
    pub geometry: fn() -> Geometry,
    pub style: SurfaceStyle,
}

/// The button a panel drops from, pressed to toggle it and drawn held while `open`: the caller makes it the control it is.
pub(crate) fn trigger(
    face: Face,
    open: RwSignal<bool>,
    toggle: Rc<dyn Fn()>,
) -> Result<StyledContainer, LayoutError> {
    let Face {
        label,
        color,
        bordered,
        caret,
        sized_by_root,
        geometry,
        style: surface,
    } = face;
    // `NoWrap`: a trigger's label is the name of a control, and splitting `File` across two lines to make room for the caret beside it turns one word into two.
    let label_text = Text::declaring(
        move || label.get(),
        LayoutStyle::new().min_width(0.0),
        |t| shared::control_text(t, 1.0).with_text_wrap(TextWrap::NoWrap),
    )?;
    // Free to shrink and clipped, so a label longer than the trigger is cut at its edge rather than widening the control or running under the caret.
    let label_text = ClippedItem::new(box_item(label_text), Clip::x());
    let paint = Rc::new(move |hovered: bool| {
        let face = Shown {
            bordered,
            open: open.get(),
            hovered,
        };
        amend_surface(trigger_rect_style(&color, face, geometry()), &surface)
    });
    let (rest, hover) = (paint.clone(), paint);
    let trigger_box = move || trigger_layout(geometry(), caret, sized_by_root);
    let mut children: Vec<Box<dyn LayoutItem>> = vec![box_item(label_text)];
    if caret {
        children.push(shared::mark(
            || shared::Mark::ChevronDown,
            |text| text.color.faded(shared::QUIET_ALPHA),
        )?);
    }
    Ok(
        StyledContainer::new(trigger_box(), move |_r| rest(false), children)?
            .styled_by(trigger_box)
            .hover_style(move |_r| hover(true))
            .on_press(move || toggle()),
    )
}

/// Builds `build`'s overlay while `open` holds, and nothing while it does not: modelled exactly like a reactive `if $open`. Building it lazily is what lets it portal correctly, since the page has been laid out by then.
pub(crate) fn while_open(
    open: RwSignal<bool>,
    build: impl Fn() -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
) -> Result<ReactiveList, LayoutError> {
    ReactiveList::new(
        move || vec![open.get()],
        |o: &bool| *o,
        move |is_open: bool| -> Result<Box<dyn LayoutItem>, LayoutError> {
            match is_open {
                true => build(),
                // Closed: an empty placeholder, with no overlay registered, so nothing blocks the page.
                false => Ok(box_item(Container::new(LayoutStyle::new(), vec![])?)),
            }
        },
        0.0,
    )
}

/// Keeps a panel on the dismiss stack while `open` holds, so Escape and the platform Back gesture reach it in the order things were opened. The stack is keyed by open and close rather than build order for exactly that reason.
pub(crate) fn dismissable(open: RwSignal<bool>, trigger: NodeId) {
    let close: Rc<dyn Fn()> = Rc::new(move || open.set(false));
    let registered: std::cell::Cell<Option<DismissRegistration>> = std::cell::Cell::new(None);
    effect(move || {
        if open.get() {
            let held = registered
                .take()
                .unwrap_or_else(|| DismissRegistration::within(trigger, close.clone()));
            registered.set(Some(held));
        } else {
            drop(registered.take());
        }
    });
}

/// Where a panel drops from its trigger: under it, `offset` away, flipping above it where there is more room there.
///
/// The offset is a memo made where the panel is built, so it is read at the control size in force there: the placement is worked out where the panel is drawn, outside any scope a subtree provided.
pub(crate) fn below(
    trigger: NodeId,
    laid: RwSignal<Rect>,
    offset: Memo<f32>,
) -> Rc<dyn Fn() -> Anchoring> {
    Rc::new(move || {
        laid.get();
        Anchoring::new(
            telar::anchor_rect(trigger, &laid),
            Placement::Below,
            popup::viewport(),
        )
        .gap(offset.get())
    })
}

/// The width a panel opens at: its own, widened to a stretched trigger's — a narrower list would read as a different control — but only ever widened: taking the trigger's width outright turned a compact `File` button into a 40px sheet with one character per line.
pub(crate) fn panel_width(own: f32, stretch: bool, trigger: NodeId, laid: &RwSignal<Rect>) -> f32 {
    match stretch {
        true => telar::anchor_rect(trigger, laid).width.max(own),
        false => own,
    }
}

/// The trigger and the overlay holder under one column. The holder's placeholder takes no space in it, so only the trigger participates in flow layout. A stretched root takes the width its row offers and none from what it shows: sized from its label, it widened past a fixed row as soon as a longer choice was picked.
pub(crate) fn root(
    trigger: StyledContainer,
    holder: ReactiveList,
    stretch: bool,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let root_box = LayoutStyle::new().flex_column();
    let root_box = match stretch {
        true => root_box.flex_grow(1.0).flex_basis(0.0).min_width(0.0),
        false => root_box,
    };
    Ok(box_item(Container::new(
        root_box,
        vec![box_item(trigger), box_item(holder)],
    )?))
}

/// The list box a `select` opens under its field: picking a row writes its index into `selected`, fires `on_pick`, and closes.
///
/// `stretch` gives the trigger the width its row offers instead of the fixed [`Geometry::width`], and the panel then opens at whatever width the trigger was laid out to. That is what a form row wants — a control of fixed width beside fields that span the row reads as a mistake — and it is the caller's choice because a select standing on its own has no row to take a width from.
pub(crate) struct Dropdown {
    /// Shown when `selected` names no row.
    pub placeholder: fn() -> String,
    /// The panel's rows, as the recipe for making them rather than the rows themselves — see [`Children`](telar::Children). Two things need it that way: the rows are remade on every open, and each one has to be built inside the [`ListContext`](crate::list::ListContext) this scaffold provides.
    pub rows: Children,
    pub color: Reactive<Color>,
    pub on_pick: Option<Rc<dyn Fn(u32)>>,
    pub selected: RwSignal<u32>,
    pub stretch: bool,
    pub geometry: fn() -> Geometry,
    pub offset: fn() -> f32,
    pub style: SurfaceStyle,
}

pub(crate) fn dropdown(props: Dropdown) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Dropdown {
        placeholder,
        rows,
        color,
        on_pick,
        selected,
        stretch,
        geometry,
        offset,
        style,
    } = props;
    let open = signal(false);
    // Not the same as what is selected: a select opens with its value under the cursor, and moving off it must not commit anything.
    let highlighted: RwSignal<Option<u32>> = signal(None);
    let pick: Rc<dyn Fn(u32)> = Rc::new(move |idx: u32| {
        selected.set(idx);
        if let Some(cb) = &on_pick {
            cb(idx);
        }
        open.set(false);
    });
    // Made here rather than per open, so the key handler — which lives as long as the trigger — asks the same registry the last build filled.
    let list = crate::list::ListContext::new(pick, highlighted, Some(selected), color.clone());
    let open_list: RwSignal<Option<FocusId>> = signal(None);
    let handback = popup::Handback::default();

    // Ask the rows what they say without asking them to be rows: the panel is not open, so there are none, and the trigger has to name the choice from the first frame.
    list.declare(&rows)?;
    let label = {
        let list = list.clone();
        Reactive::of(move || list.label_of(selected.get()).unwrap_or_else(placeholder))
    };
    // Key events are broadcast, so without this a bare Enter would open every select on the page.
    let trigger_focused = signal(false);
    let toggle: Rc<dyn Fn()> = Rc::new(move || {
        let opening = !open.peek();
        if opening {
            // The cursor starts on what is already chosen, so the first arrow moves from there.
            highlighted.set(Some(selected.peek()));
        }
        open.set(opening);
    });
    let face = Face {
        label,
        color: color.clone(),
        bordered: true,
        caret: true,
        sized_by_root: stretch,
        geometry,
        style,
    };
    // One handler on the trigger, which lives for the widget's whole life. The panel is rebuilt on every open and would lose a handler hung on it.
    let on_key = {
        let toggle = toggle.clone();
        let list = list.clone();
        move |key: &Key| -> bool {
            let is_open = open.peek();
            let mods = telar::modifiers();
            let row_count = list.len();
            match key {
                // `dispatch_overlays` only dismisses when nothing holds focus — right for a field, which blurs itself first, and wrong here, where the focused thing is the control the list belongs to.
                Key::Named(NamedKey::Escape) if is_open => open.set(false),
                Key::Named(NamedKey::ArrowDown) if is_open => {
                    highlighted.set(list.step(highlighted.peek(), 1))
                }
                Key::Named(NamedKey::ArrowUp) if is_open => {
                    highlighted.set(list.step(highlighted.peek(), -1))
                }
                // The first and last rows the cursor may stop on, which is not the same as the first and last rows: a list opening with a group heading has neither at 0.
                Key::Named(NamedKey::Home) if is_open && row_count > 0 => {
                    highlighted.set(list.edge(1))
                }
                Key::Named(NamedKey::End) if is_open && row_count > 0 => {
                    highlighted.set(list.edge(-1))
                }
                Key::Named(NamedKey::Enter) if is_open => {
                    if let Some(idx) = highlighted.peek() {
                        list.commit(idx);
                    }
                }
                // Ctrl or Meta makes a character a command rather than a query, so Ctrl+S must not walk to the first row starting with "s". Alt is left out, as everywhere here: on many layouts it is how a character is typed.
                Key::Char(c) if is_open && !c.is_control() && !mods.is_ctrl && !mods.is_meta => {
                    if let Some(idx) = list.type_ahead(*c, highlighted.peek()) {
                        highlighted.set(Some(idx));
                    }
                }
                // A space is a character only inside a query already under way, and the opener every other time.
                Key::Named(NamedKey::Space) if is_open && list.is_searching() => {
                    if let Some(idx) = list.type_ahead(' ', highlighted.peek()) {
                        highlighted.set(Some(idx));
                    }
                }
                Key::Named(NamedKey::ArrowDown | NamedKey::Enter | NamedKey::Space)
                    if !is_open && trigger_focused.peek() =>
                {
                    toggle()
                }
                _ => return false,
            }
            // Taken, so the list the panel scrolls does not also answer an arrow by scrolling when it holds focus.
            true
        }
    };
    let trigger = {
        let list = list.clone();
        trigger(face, open, toggle)?
            // A control, which is what makes every key above addressable. It answers its own keys, so the default Enter-activates-the-press is stood down.
            .control(Role::ComboBox)
            .has_popup(HasPopup::ListBox)
            .expanded(move || open.get())
            .on_focus(move |now| trigger_focused.set(now))
            .on_key(on_key)
            .consumes_keys(move || {
                if open.get() {
                    ConsumedKeys::VERTICAL_ARROWS | ConsumedKeys::EDGES | ConsumedKeys::ACTIVATION
                } else {
                    ConsumedKeys::ARROW_DOWN | ConsumedKeys::ACTIVATION
                }
            })
            .controls(move || open_list.get())
            // A select keeps the keyboard while its list is open, so it is what points a reader at the row the cursor is on.
            .active_descendant(move || {
                open.get()
                    .then(|| highlighted.get())
                    .flatten()
                    .and_then(|i| list.focus_of(i))
            })
    };
    let trigger_node = trigger.layout_node();
    // `track_layout` returns the signal now and layout fills it in, so the trigger's rect is known by the time the panel is built.
    let trigger_rect = track_layout(trigger_node).expect("trigger node is registered");

    let remembering = handback.clone();
    let holder = while_open(open, move || {
        // The registry is cleared first: this builder runs again on every open, and rows left over from the last would leave the keyboard walking positions that no longer exist.
        list.begin();
        let row_items = rows.build_with(list.clone())?.take_default();
        let width = panel_width(geometry().width, stretch, trigger_node, &trigger_rect);
        let row_column = Container::new(LayoutStyle::new().flex_column(), row_items)?;
        let cursor = {
            let list = list.clone();
            Rc::new(move || highlighted.get().and_then(|i| list.node_of(i)))
        };
        let placed = popup::Popup {
            anchoring: below(trigger_node, trigger_rect, memo(offset)),
            width: popup::Width::Exactly(width),
            padding: shared::panel_pad(),
            cursor,
            keyboard: popup::Keyboard::Stays,
        };
        let opened = popup::popup(placed, box_item(row_column), |_r| panel_rect_style())?;
        let (panel, opened_list) = (opened.panel, opened.list);
        open_list.set(Some(opened_list));
        // A transparent full-viewport backdrop inside a blocking overlay: stops hover and clicks bleeding through to the page behind, and dismisses when a click lands outside the panel.
        let backdrop = StyledContainer::new(
            LayoutStyle::new().flex_column().flex_grow(1.0),
            |_r| RectStyle::default(),
            vec![box_item(panel)],
        )?;
        let backdrop = shared::dismissed_by_press(backdrop, move || open.set(false));
        let overlay = Overlay::new(LayoutStyle::new().flex_column(), vec![box_item(backdrop)])?
            .owned_by(trigger_node);
        remembering.remember(overlay.content_node());
        Ok(box_item(overlay))
    })?;

    // Every way of closing comes through `open`, and each has to leave the keyboard on the field, where it was before the list opened.
    effect(move || {
        if !open.get() {
            handback.give_back();
            if open_list.peek().is_some() {
                open_list.set(None);
            }
        }
    });
    dismissable(open, trigger_node);
    root(trigger, holder, stretch)
}

/// The trigger's box: as tall as every control, padded by its tokens, and as wide as they say unless the root column decides its width.
///
/// A stretched or fitted trigger is as wide as the root column by cross-axis stretch, which a fitted root takes from the label. Growing it would grow it along that column's main axis, which is its height.
///
/// With a caret, `SPACE_BETWEEN` rather than a growing spacer: a spacer takes the free space before the label has claimed its own, and `File` came out wrapped to two letters a line. Without one the label is the whole face and stands centred, as a button's does: alone in a `SPACE_BETWEEN` row it sat at the start, and every pixel the trigger had beyond it showed as a gap after the label and none before.
fn trigger_layout(geometry: Geometry, with_caret: bool, sized_by_root: bool) -> LayoutStyle {
    let justify = if with_caret {
        JustifyContent::SPACE_BETWEEN
    } else {
        JustifyContent::CENTER
    };
    let row = LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(justify)
        .min_height(shared::control_height())
        .padding_horizontal(geometry.pad_x)
        .gap(shared::gap_sm());
    if sized_by_root {
        row
    } else {
        row.width(geometry.width)
    }
}

/// What the trigger's face is showing.
#[derive(Clone, Copy)]
struct Shown {
    bordered: bool,
    open: bool,
    hovered: bool,
}

/// A select is a form control and wears a border; a **menu** is a button that happens to open a list, so it wears none. A bordered menu trigger reads as a field with the caret in it — or, sitting alone in a header, as a button stuck in its focused state. `bordered` is there for the caller who wants the other reading.
fn trigger_rect_style(color: &Reactive<Color>, face: Shown, geometry: Geometry) -> RectStyle {
    let rect = RectStyle::default().with_radius(BorderRadius::all(geometry.radius));
    if !face.bordered {
        return match (face.open, face.hovered) {
            (true, _) => rect.with_fill(shared::held_wash()),
            (false, true) => rect.with_fill(shared::hover_wash()),
            (false, false) => rect,
        };
    }
    let accent = shared::resolve(color, shared::accent);
    rect.with_fill(shared::surface())
        .with_border(Border::uniform(accent, geometry.border_width))
}

fn panel_rect_style() -> RectStyle {
    RectStyle::default()
        .with_fill(shared::surface())
        .with_border(Border::uniform(shared::border(), shared::border_width()))
        .with_radius(BorderRadius::all(shared::radius_surface()))
}

#[cfg(test)]
#[path = "dropdown_test.rs"]
mod tests;
