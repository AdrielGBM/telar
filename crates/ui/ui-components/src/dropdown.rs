//! Shared dropdown scaffold (constants, style fns, trigger + overlay/backdrop/anchoring) behind `menu` and `select`.

use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{AlignItems, LayoutError, LayoutStyle};
use platform_core::{Key, NamedKey};
use reactive_core::{RwSignal, effect, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke, TextStyle};
use ui_core::focus::Role;
use ui_core::{
    Children, Container, LayoutItem, Overlay, ReactiveList, StyledContainer, Text, box_item,
    track_layout,
};

use crate::shared;

/// The trigger's fixed size and the dropdown panel's geometry. Kept as constants (not content-sized) so the
/// panel lines up under the trigger and so the widgets' hit geometry is deterministic (their tests compute
/// click points from these).
pub(crate) const PANEL_WIDTH: f32 = 180.0;
/// Gap kept between the panel and the window edge it was pushed off.
const EDGE_MARGIN: f32 = 4.0;
pub(crate) const TRIGGER_HEIGHT: f32 = 36.0;
pub(crate) const ROW_HEIGHT: f32 = 32.0;
pub(crate) fn panel_pad() -> f32 {
    shared::spacing() * 0.5
}

/// What the trigger says, which is the one place a `menu` and a `select` genuinely differ. A menu commits
/// actions, so its trigger is a name the caller fixes; a select holds a choice, so its trigger *is* that
/// choice — and has to say it before the panel has ever been opened.
pub(crate) enum TriggerLabel {
    Fixed(Box<dyn Fn() -> String>),
    /// Whatever the chosen row says it is, falling back to `placeholder` for an index naming no row.
    Selected {
        placeholder: &'static str,
    },
}

/// The trigger + anchored-panel scaffold shared by `menu` and `select`: a bordered trigger button opens a
/// blocking overlay whose transparent backdrop dismisses on click-away, and whose anchored panel lists `rows`;
/// picking a row optionally writes into `selected`, fires `on_pick`, and closes.
///
/// `trigger_label` is a closure so `select` can track its bound signal reactively while `menu` supplies a
/// static label. `selected` drives the bound-selection behaviour: `Some` writes the picked index back and
/// highlights the selected row; `None` (menu) skips both, leaving one-shot actions.
///
/// `stretch` gives the trigger the width its row offers instead of the fixed [`PANEL_WIDTH`], and the panel then
/// opens at whatever width the trigger was laid out to. That is what a form row wants — a control 180px wide
/// beside fields that span the row reads as a mistake — and it is the caller's choice because a dropdown
/// standing on its own has no row to take a width from.
///
/// Everything the scaffold is told, as one value. It outgrew what a positional argument list can be read at
/// a call site — and the two shape flags are the *caller's* to make rather than the component's.
pub(crate) struct Dropdown {
    pub label: TriggerLabel,
    /// The panel's rows, as the recipe for making them rather than the rows themselves — see
    /// [`Children`](ui_core::Children). Two things need it that way: the rows are remade on every open, and
    /// each one has to be built inside the [`ListContext`](crate::list::ListContext) this scaffold provides.
    pub rows: Children,
    pub color: Box<dyn Fn() -> Color>,
    pub on_pick: Option<Box<dyn Fn(u32)>>,
    pub selected: Option<RwSignal<u32>>,
    pub stretch: bool,
    pub bordered: bool,
    pub caret: bool,
    pub style: Option<Box<dyn Fn(RectStyle) -> RectStyle>>,
}

pub(crate) fn dropdown(props: Dropdown) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Dropdown {
        label: trigger_label,
        rows,
        color,
        on_pick,
        selected,
        stretch,
        bordered,
        caret: with_caret,
        style: surface,
    } = props;
    let surface: shared::SurfaceStyle =
        surface.map(|f| -> Rc<dyn Fn(RectStyle) -> RectStyle> { Rc::from(f) });
    // Erased to `Rc` so the colour/callback can be cloned into the trigger, every option row, and the panel
    // builder (which re-runs on each open) — a `Box<dyn Fn>` can't be shared, but the widget needs to.
    let color: shared::ReactiveColor = Rc::from(color);
    let on_pick: Option<Rc<dyn Fn(u32)>> = on_pick.map(|f| -> Rc<dyn Fn(u32)> { Rc::from(f) });
    let open = signal(false);
    let dismiss_open = open.clone();
    // Not the same thing as what is selected: a bound `select` opens with its value under the cursor, and moving off it must not commit anything.
    let highlighted: RwSignal<Option<u32>> = signal(None);
    // Committing a row, built once so Enter and a tap take the same path instead of two that drift.
    let pick: Rc<dyn Fn(u32)> = {
        let selected = selected.clone();
        let on_pick = on_pick.clone();
        let open = open.clone();
        Rc::new(move |idx: u32| {
            if let Some(sel) = &selected {
                sel.set(idx);
            }
            if let Some(cb) = &on_pick {
                cb(idx);
            }
            open.set(false);
        })
    };
    // What the rows are built inside, and what they register themselves with. Made here rather than per open
    // so the key handler — which lives as long as the trigger — asks the same registry the last build filled.
    let list = crate::list::ListContext::new(
        pick.clone(),
        highlighted.clone(),
        selected.clone(),
        color.clone(),
    );

    let trigger_label: Box<dyn Fn() -> String> = match trigger_label {
        TriggerLabel::Fixed(fixed) => fixed,
        TriggerLabel::Selected { placeholder } => {
            // Ask the rows what they say without asking them to be rows — the panel has not been opened, so
            // there are none, and this trigger has to name the choice from the first frame.
            list.declare(&rows)?;
            let list = list.clone();
            let selected = selected.clone();
            Box::new(move || {
                selected
                    .as_ref()
                    .and_then(|s| list.label_of(s.get()))
                    .unwrap_or_else(|| placeholder.to_string())
            })
        }
    };
    // Trigger: a bordered button with the label; a tap toggles `open`.
    // `auto` (measured) so the label has width in this row; `single_line` only sets height → width 0 → invisible.
    // `no_wrap`: a trigger's label is the *name* of a control, and splitting `File` across two lines to make
    // room for the caret beside it turns one word into two.
    let label_text = Text::new(trigger_label, LayoutStyle::new(), || {
        TextStyle::new(shared::font_size(), shared::ink()).with_no_wrap(true)
    })?;
    let trigger_style = {
        let color = color.clone();
        let surface = surface.clone();
        move |_r: Rect| shared::amend(trigger_rect_style(color.as_ref(), bordered), &surface)
    };
    // Key events are broadcast, so without this a bare Enter or Down would open every dropdown on the page rather than this one.
    let trigger_focused = signal(false);
    let toggle: Rc<dyn Fn()> = {
        let open = open.clone();
        let highlighted = highlighted.clone();
        let selected = selected.clone();
        Rc::new(move || {
            let opening = !open.peek();
            if opening {
                // The cursor starts on what is already chosen, so the first arrow moves from there rather than from the top of a list the user is partway down.
                highlighted.set(selected.as_ref().map(|s| s.peek()));
            }
            open.set(opening);
        })
    };
    let trigger_box = LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .height(TRIGGER_HEIGHT)
        .padding_horizontal(12.0)
        .gap(6.0)
        .justify_content(layout_core::JustifyContent::SPACE_BETWEEN);
    let trigger_box = if stretch {
        trigger_box.flex_grow(1.0)
    } else {
        trigger_box.width(PANEL_WIDTH)
    };
    // Pushed apart with `SPACE_BETWEEN` and not with a growing spacer: a spacer takes the free space before
    // the label has claimed its own, and `File` came out wrapped to two letters a line.
    let mut trigger_children: Vec<Box<dyn LayoutItem>> = vec![box_item(label_text)];
    if with_caret {
        trigger_children.push(caret()?);
    }
    // One handler on the trigger, which lives for the widget's whole life and asks `open` what state it is in — the panel is rebuilt on every open and would lose a handler hung on it.
    let on_key = {
        let open = open.clone();
        let highlighted = highlighted.clone();
        let pick = pick.clone();
        let toggle = toggle.clone();
        let trigger_focused = trigger_focused.clone();
        let list = list.clone();
        move |key: &Key| {
            let is_open = open.peek();
            let mods = ui_core::modifiers();
            // The registry the last build filled, so the cursor walks the rows that exist and steps over the
            // ones it may not stop on — a disabled row, a separator, a group heading.
            let step = |from: Option<u32>, delta: i64| list.step(from, delta);
            let row_count = list.len();
            match key {
                // `dispatch_overlays` only dismisses when nothing holds focus — right for a field, which blurs itself first, and wrong here, where the focused thing *is* the control the dropdown belongs to.
                Key::Named(NamedKey::Escape) if is_open => open.set(false),
                Key::Named(NamedKey::ArrowDown) if is_open => {
                    highlighted.set(step(highlighted.peek(), 1))
                }
                Key::Named(NamedKey::ArrowUp) if is_open => {
                    highlighted.set(step(highlighted.peek(), -1))
                }
                // The ends of the list are the first and last rows the cursor may stop on, which is not the
                // same as the first and last rows: a menu that opens with a group heading has neither at 0.
                Key::Named(NamedKey::Home) if is_open && row_count > 0 => {
                    highlighted.set(list.edge(1))
                }
                Key::Named(NamedKey::End) if is_open && row_count > 0 => {
                    highlighted.set(list.edge(-1))
                }
                Key::Named(NamedKey::Enter) if is_open => {
                    if let Some(idx) = highlighted.peek() {
                        pick(idx);
                    }
                }
                // Type-ahead. Ctrl or Meta makes a character a command rather than a query, so Ctrl+S must not
                // walk to the first row starting with *s*. Alt is left out of that test as everywhere else
                // here: on a good many layouts it is how you type a character in the first place.
                Key::Char(c) if is_open && !c.is_control() && !mods.is_ctrl && !mods.is_meta => {
                    if let Some(idx) = list.type_ahead(*c, highlighted.peek()) {
                        highlighted.set(Some(idx));
                    }
                }
                // A space is a character only inside a query already under way — `Show ruler` needs it to be
                // one — and the opener above every other time, which is what a closed trigger answers to.
                Key::Named(NamedKey::Space) if is_open && list.is_searching() => {
                    if let Some(idx) = list.type_ahead(' ', highlighted.peek()) {
                        highlighted.set(Some(idx));
                    }
                }
                // Shut, and the trigger is where the keyboard is: the openers a native control answers to.
                Key::Named(NamedKey::ArrowDown | NamedKey::Enter | NamedKey::Space)
                    if !is_open && trigger_focused.peek() =>
                {
                    toggle();
                    // Opening *downward* lands on the first row, as a native control does. `toggle` only seeds from an existing selection, because opening with the mouse must highlight nothing — a cursor nobody moved reads as a hover that is not happening.
                    // Asked for rather than set: the rows do not exist yet — the panel is built on the flush
                    // after this handler returns — so the first one that can take the cursor claims it there.
                    if matches!(key, Key::Named(NamedKey::ArrowDown))
                        && highlighted.peek().is_none()
                    {
                        list.seed_cursor();
                    }
                }
                _ => {}
            }
        }
    };
    let trigger = StyledContainer::new(trigger_box, trigger_style, trigger_children)?
        .on_press({
            let toggle = toggle.clone();
            move || toggle()
        })
        // A control, which is what makes every key above addressable — and what a reader needs to say this
        // opens a list. It answers its own keys, so the default Enter-activates-the-press is stood down.
        .control(if selected.is_some() {
            Role::ComboBox
        } else {
            Role::Button
        })
        .on_focus(move |now| trigger_focused.set(now))
        .on_key(on_key);
    let trigger_node = trigger.layout_node();
    // The trigger's laid-out rect positions the panel; `track_layout` returns the signal now (default rect)
    // and layout fills it in, so by the time the panel is built (on open) the trigger's rect is known.
    let trigger_rect = track_layout(trigger_node).expect("trigger node is registered");

    // Open/close is modelled exactly like a reactive `if $open`: a single-item `ReactiveList` keyed on the
    // boolean. Building the overlay lazily (only while open) is what lets it portal correctly — the page has
    // been laid out by then, so the overlay attaches to the viewport host and the trigger's rect is known.
    let overlay_holder = ReactiveList::new(
        {
            let open = open.clone();
            move || vec![open.get()]
        },
        |o: &bool| *o,
        move |is_open: bool| -> Result<Box<dyn LayoutItem>, LayoutError> {
            if !is_open {
                // Closed: an empty placeholder (no overlay registered, so nothing blocks the page).
                return Ok(box_item(Container::new(LayoutStyle::new(), vec![])?));
            }
            // The rows make themselves, inside the context that tells each one where it sits and what to
            // commit. The registry is cleared first: this builder runs again on every open, and rows left
            // over from the last one would leave the keyboard walking positions that no longer exist.
            list.begin();
            let row_items = rows.build_with(list.clone())?.take_default();
            // Place the definite-width panel at the trigger's bottom-left via layout margins; positioning by
            // layout keeps the panel's hit rects in world space so the rows dispatch correctly. The trigger's
            // rect is window-absolute (the overlay hoists to the window), falling back to the local rect.
            let anchor = ui_core::overlay::anchor_rect(trigger_node, &trigger_rect);
            // The anchor is the trigger's rect as it stands when the panel opens: a re-resolve on a theme
            // switch keeps the margins it was built from, which is where the panel belongs either way.
            // A filled trigger widens the panel to match — it opens under a control the row sized, so a
            // narrower list would read as a different control — but only ever *widens* it. Taking the
            // trigger's width outright is what turned a compact `File` button into a 40px sheet with one
            // character per line; the trigger is a floor, never a ceiling.
            let width = if stretch {
                anchor.width.max(PANEL_WIDTH)
            } else {
                PANEL_WIDTH
            };
            // And it has to land on screen: a trigger near the right edge would otherwise open a panel that
            // runs past it, and one near the bottom would open below the window.
            let viewport =
                ui_core::overlay_viewport().unwrap_or(Rect::new(0.0, 0.0, f32::MAX, f32::MAX));
            let left = (anchor.x)
                .min(viewport.x + viewport.width - width - EDGE_MARGIN)
                .max(viewport.x);
            let below = anchor.y + anchor.height;
            let sheet = move || {
                LayoutStyle::new()
                    .flex_column()
                    .width(width)
                    .padding_all(panel_pad())
                    .margin_from_left(left)
                    .margin_block_start(below)
            };
            let panel = StyledContainer::new(sheet(), |_r| panel_rect_style(), row_items)?
                .styled_by(sheet)
                // Swallow clicks on the panel's own padding so they don't dismiss via the backdrop.
                .on_press(|| {});
            // A transparent full-viewport backdrop inside a BLOCKING overlay: stops hover/clicks bleeding
            // through to the page behind and dismisses the dropdown when a click lands outside the panel.
            let close = {
                let open = open.clone();
                move || open.set(false)
            };
            let backdrop = StyledContainer::new(
                LayoutStyle::new().flex_column().flex_grow(1.0),
                |_r| RectStyle::default(),
                vec![box_item(panel)],
            )?
            .on_press(close);
            let overlay = Overlay::new(LayoutStyle::new().flex_column(), vec![box_item(backdrop)])?;
            Ok(box_item(overlay))
        },
        0.0,
    )?;

    // On the dismiss stack while up, so Escape and the platform Back gesture reach it in the order the user opened things — a menu over a dialog closes the menu first.
    // The stack is keyed by open/close rather than by build order for exactly that reason, and this widget was never on it: the only way to shut a dropdown was a click on its backdrop.
    let dismiss_tracker = {
        let open = dismiss_open;
        let close: Rc<dyn Fn()> = {
            let open = open.clone();
            Rc::new(move || open.set(false))
        };
        let registered: std::cell::Cell<Option<ui_core::dismiss::DismissId>> =
            std::cell::Cell::new(None);
        effect(move || {
            if open.get() {
                if registered.get().is_none() {
                    registered.set(Some(ui_core::dismiss::register_dismiss(close.clone())));
                }
            } else if let Some(id) = registered.take() {
                ui_core::dismiss::unregister_dismiss(id);
            }
        })
    };

    // The widget owns both the trigger and the (portaling) overlay holder; the holder's placeholder takes no
    // space in the column, so only the trigger participates in flow layout.
    // `stretch` has to reach the root as well as the trigger: the trigger grows inside *this* box, and this box is what the caller's row lays out — left content-sized it shrinks to the label and the trigger with it.
    let root_box = LayoutStyle::new().flex_column();
    let root_box = if stretch {
        root_box.flex_grow(1.0)
    } else {
        root_box
    };
    let root = Container::new(root_box, vec![box_item(trigger), box_item(overlay_holder)])?
        .keeping(dismiss_tracker);
    Ok(box_item(root))
}

/// A select is a form control and wears a border; a **menu** is a button that happens to open a list, so it
/// wears none. A bordered menu trigger reads as a field with the caret in it — or, sitting alone in a header,
/// as a button stuck in its focused state. `bordered` is there for the caller who wants the other reading.
fn trigger_rect_style(color: &dyn Fn() -> Color, bordered: bool) -> RectStyle {
    let radius = BorderRadius::all(shared::radius_md());
    if !bordered {
        return RectStyle::default().with_radius(radius);
    }
    let accent = shared::resolve(color, shared::accent);
    RectStyle::default()
        .with_fill(shared::surface())
        .with_stroke(Stroke::new(accent, 1.0))
        .with_radius(radius)
}

/// The caret every trigger carries, drawn rather than spelled: a glyph would depend on the face having it
/// and on it being the size the label is. Two strokes, 11px across: wide enough to read beside a label at the
/// body size, narrow enough not to compete with it.
fn caret() -> Result<Box<dyn LayoutItem>, LayoutError> {
    const W: f32 = 11.0;
    const H: f32 = 5.5;
    let canvas = ui_core::Canvas::new(LayoutStyle::new().width(W).height(H), |rect| {
        let top = (rect.height - H) / 2.0;
        let data = std::sync::Arc::new(
            renderer_core::PathData::new()
                .move_to(geometry_core::Point::new(0.0, top))
                .line_to(geometry_core::Point::new(W / 2.0, top + H))
                .line_to(geometry_core::Point::new(W, top)),
        );
        ui_core::RenderNode::path(
            data,
            renderer_core::PathStyle::default()
                .with_stroke(Stroke::new(shared::ink().with_alpha(0.6), 1.4)),
        )
    })?;
    Ok(box_item(canvas))
}

fn panel_rect_style() -> RectStyle {
    RectStyle::default()
        .with_fill(shared::surface())
        .with_stroke(Stroke::new(shared::border(), 1.0))
        .with_radius(BorderRadius::all(shared::radius()))
}

// A menu row (`selected: None` → always `is_selected == false`) yields the plain radius-only style; a select
// row highlights the bound choice with a faint accent fill.
pub(crate) fn option_row_style(is_selected: bool, color: &dyn Fn() -> Color) -> RectStyle {
    let radius = BorderRadius::all(shared::radius_sm());
    if is_selected {
        let accent = shared::resolve(color, shared::accent);
        RectStyle::default()
            .with_fill(accent.with_alpha(0.14))
            .with_radius(radius)
    } else {
        RectStyle::default().with_radius(radius)
    }
}

pub(crate) fn option_row_hover_style(color: &dyn Fn() -> Color) -> RectStyle {
    let accent = shared::resolve(color, shared::accent);
    RectStyle::default()
        .with_fill(accent.with_alpha(0.10))
        .with_radius(BorderRadius::all(4.0))
}
