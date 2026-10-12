//! The menu a gesture asks for, wherever it was asked.
//!
//! Not [`menu`](crate::menu), which is a labelled button that drops a list under itself. This one has no trigger at all: it opens at a point, because the thing it is about is what was under the pointer. That is the difference the catalogue was missing, and every application that needed one wrote the panel, the placement, the dismissal and the keyboard again — and stopped at the panel and the placement, which is why a context menu almost never answers an arrow key.
//!
//! **What it owns:** where the panel goes and that it stays on screen, which row is highlighted, the keys that move and commit, the submenu that opens beside a row, and every way of closing it. **What it does not own:** what the rows are, what they do, and what they look like — an entry carries its own action, a caller can hand over a whole widget for a row that is not a line of text, and the panel's paint is amendable.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use telar::{
    AlignItems, AnchorAlign, Anchoring, Children, Color, Container, JustifyContent, Key,
    LayoutError, LayoutItem, LayoutStyle, NamedKey, NodeId, Overlay, Placement, PointerButton,
    Props, Rect, RectStyle, RwSignal, ScrollViewport, SizeDimension, StyledContainer, Text,
    TextStyle, box_item, effect,
    focus::{self, FocusId, Role},
    on_cleanup, signal, track_layout,
};

use crate::popup;
use crate::shared;

/// One line of a menu.
///
/// The action rides on the entry rather than coming back as an index, because the rows of a context menu are heterogeneous by nature — half of them are only there when they apply — and a caller matching on numbers keeps a second list in step with the first for no reason.
#[derive(Clone)]
pub enum Entry {
    /// A label, the key that does the same thing, and what picking it does.
    Row {
        label: String,
        /// Drawn faintly at the trailing edge: the shortcut this row is the discoverable half of. Empty for an action with no key.
        hint: String,
        act: Rc<dyn Fn()>,
        enabled: bool,
    },
    /// A row that opens another menu beside it.
    Sub { label: String, entries: Vec<Entry> },
    /// A row the caller draws: the strip of icons a file manager puts at the top of its menu, a colour swatch, a preview. `act` makes it a stop for the keyboard as well as for the pointer; without one the arrows step over it, which is what a heading or a self-contained strip of buttons wants.
    ///
    /// A recipe rather than a built widget, so a menu can be opened twice — and so the entry list is `Clone`, which is what lets it ride on props that reach a region that rebuilds.
    Custom {
        widget: Rc<dyn Fn() -> Result<Box<dyn LayoutItem>, LayoutError>>,
        act: Option<Rc<dyn Fn()>>,
    },
    /// A line between two groups of rows. Never a stop.
    Separator,
}

impl Entry {
    /// A plain row with a shortcut hint.
    pub fn row(
        label: impl Into<String>,
        hint: impl Into<String>,
        act: impl Fn() + 'static,
    ) -> Self {
        Entry::Row {
            label: label.into(),
            hint: hint.into(),
            act: Rc::new(act),
            enabled: true,
        }
    }

    /// The same, greyed and unpickable — there, and unavailable, which is a different thing from absent.
    pub fn disabled(self) -> Self {
        match self {
            Entry::Row {
                label, hint, act, ..
            } => Entry::Row {
                label,
                hint,
                act,
                enabled: false,
            },
            other => other,
        }
    }

    /// Whether the keyboard stops here.
    fn stops(&self) -> bool {
        match self {
            Entry::Row { enabled, .. } => *enabled,
            Entry::Sub { .. } => true,
            Entry::Custom { act, .. } => act.is_some(),
            Entry::Separator => false,
        }
    }
}

/// How a menu is painted, and how wide. Every colour is the caller's: this crate has no palette of its own and a context menu is chrome, which is exactly where an application's own look shows.
#[derive(Clone, Copy)]
pub struct MenuStyle {
    pub background: Color,
    pub border: Color,
    pub label: Color,
    /// The shortcut, and a disabled row's label.
    pub faint: Color,
    /// Behind the row under the pointer or the keyboard.
    pub highlight: Color,
    pub radius: f32,
    pub font_size: f32,
    /// The height of a row the component draws itself, which is at least a pointer target tall by default. A `Custom` row is whatever its widget measures.
    pub row_height: f32,
    pub padding: f32,
    /// The mark a row wears to say it opens another panel.
    ///
    /// A setting and not a constant, because it is a character: a face without it draws whatever the desktop offers in its place, at a size meant for another grid, and says nothing about having done so.
    pub submenu: &'static str,
}

impl Default for MenuStyle {
    fn default() -> Self {
        Self {
            background: Color::rgba(0.10, 0.10, 0.12, 1.0),
            border: Color::rgba(0.25, 0.25, 0.28, 1.0),
            label: Color::rgba(0.90, 0.90, 0.92, 1.0),
            faint: Color::rgba(0.55, 0.55, 0.58, 1.0),
            highlight: Color::rgba(0.20, 0.20, 0.24, 1.0),
            radius: 4.0,
            font_size: 13.0,
            row_height: shared::MIN_TARGET,
            padding: 4.0,
            submenu: "›",
        }
    }
}

/// The list a menu's children push themselves into as they build.
///
/// A context menu's rows are heterogeneous and half of them are only there when they apply, which in markup is an `if` around a row and in Rust was a `Vec` built with pushes. The pieces below register what they are and hand back a bare node, the way a bound list's pieces do for [`ListContext::declare`](crate::list): the panel still owns the highlight, the keyboard and the submenus, because it still has the entries — they are simply written where they are read now.
#[derive(Clone)]
struct MenuEntries(Rc<std::cell::RefCell<Vec<Entry>>>);

/// Runs `children` for their entries alone, and takes what they handed back out of the tree: a piece that registered itself is not a widget.
fn declared(children: &Children) -> Result<Vec<Entry>, LayoutError> {
    let collected = MenuEntries(Rc::default());
    let mut built = children.build_with(collected.clone())?;
    // Freeing each root frees every piece beneath it, however the pieces were wrapped.
    for item in built.take_default() {
        telar::remove_node(item.layout_node());
    }
    Ok(collected.0.take())
}

/// Registers one entry with the menu being built, and hands back the bare node every piece returns.
fn register(entry: Entry) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Some(collected) = telar::use_context::<MenuEntries>() else {
        // Silence here would be a row that vanished: the author wrote it, nothing drew it, and nothing said why.
        return Err(LayoutError::Engine(
            "a menu row belongs inside a `context_menu`, which is what collects it".to_string(),
        ));
    };
    let bare = StyledContainer::new(LayoutStyle::new(), |_| RectStyle::default(), vec![])?;
    collected.0.borrow_mut().push(entry);
    Ok(box_item(bare))
}

#[derive(Props)]
/// One selectable row of a context menu.
pub struct MenuRowProps {
    #[props(into)]
    pub label: String,
    /// Drawn faintly at the trailing edge: the key that does the same thing, which is the half of a shortcut anybody ever discovers.
    #[props(into, default)]
    pub hint: String,
    #[props(default = Rc::new(|| {}))]
    pub on_select: Rc<dyn Fn()>,
    /// There and unavailable, which is a different thing from absent.
    #[props(default = false)]
    pub disabled: bool,
}

/// One line of a menu: what it says, the key that says it too, and what picking it does.
pub fn menu_row(
    props: MenuRowProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let row = Entry::Row {
        label: props.label,
        hint: props.hint,
        act: props.on_select,
        enabled: !props.disabled,
    };
    register(row)
}

#[derive(Props)]
/// A rule between groups of rows.
pub struct MenuSeparatorProps {}

/// A line between two groups of rows. Never a stop for the keyboard.
pub fn menu_separator(
    _props: MenuSeparatorProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    register(Entry::Separator)
}

#[derive(Props)]
/// A row that opens a submenu beside it.
pub struct MenuSubProps {
    #[props(into)]
    pub label: String,
}

/// A row that opens another menu beside it, whose rows are this one's children.
pub fn menu_sub(
    props: MenuSubProps,
    children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    register(Entry::Sub {
        label: props.label,
        entries: declared(&children)?,
    })
}

#[derive(Props)]
/// A row whose content is whatever markup the caller nested inside it.
pub struct MenuCustomProps {
    /// What picking it does, and whether the arrows stop on it at all: without one the keyboard steps over it, which is what a heading or a self-contained strip of buttons wants.
    #[props(some, default)]
    pub on_select: Option<Rc<dyn Fn()>>,
}

/// A row the caller draws: a heading, a strip of icons, a swatch — whatever the children are.
pub fn menu_custom(
    props: MenuCustomProps,
    children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // The children are a recipe, which is what a `Custom` entry wants: a menu can be opened twice.
    let widget = Rc::new(move || {
        let mut items = children.build()?.take_default();
        match items.len() {
            1 => Ok(items.remove(0)),
            _ => Ok(box_item(StyledContainer::new(
                LayoutStyle::new().flex_column(),
                |_| RectStyle::default(),
                items,
            )?)),
        }
    });
    register(Entry::Custom {
        widget,
        act: props.on_select,
    })
}

#[derive(Props)]
/// The menu itself: the rows it holds and where it is anchored.
pub struct ContextMenuProps {
    /// Where it was asked for, in surface coordinates.
    #[props(default = (0.0, 0.0))]
    pub at: (f32, f32),
    #[props(default = Vec::new())]
    pub entries: Vec<Entry>,
    /// Run when the menu is done with: a row was picked, Escape was pressed, or the hand went elsewhere. The caller owns whether the menu exists, so this is how it says the answer is in.
    #[props(default = Rc::new(|| {}))]
    pub on_close: Rc<dyn Fn()>,
    #[props(default = 180.0)]
    pub width: f32,
    /// The box the panel is kept inside. The window, usually; a pane for a menu that belongs to one.
    #[props(default = Rect::new(0.0, 0.0, f32::MAX, f32::MAX))]
    pub within: Rect,
    #[props(default = MenuStyle::default())]
    pub style: MenuStyle,
}

/// A menu opened at a point: rows, submenus, the keyboard, and every way out.
///
/// The rows are its children — `menu_row`, `menu_separator`, `menu_sub`, `menu_custom` — or the `entries` prop, which is the same list handed over already built. Children come after, so a caller may do both.
pub fn context_menu(
    props: ContextMenuProps,
    children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ContextMenuProps {
        at,
        entries,
        on_close,
        width,
        within,
        style,
    } = props;
    let mut entries = entries;
    entries.extend(declared(&children)?);
    let closing = Rc::new(on_close);
    let point = Rect::new(at.0, at.1, 0.0, 0.0);
    let anchoring: Anchored =
        Rc::new(move || Anchoring::new(point, Placement::Below, kept_inside(within)));
    let menu = Built::panel(entries, anchoring, width, within, style, closing.clone())?;
    let (keys, list, deepest) = (menu.keys, menu.list, menu.deepest);
    let mut layers = vec![menu.panel];
    layers.extend(menu.layers);

    // A see-through sheet inside a blocking overlay, so the press never reaches whatever the menu is about. It closes on the stroke rather than the tap: a drag with no threshold reports from the press itself, so this fires the moment the button goes down, which also covers the hand that presses away and keeps moving. Waiting for a release a drag never delivers is how a menu ends up standing over a window nobody asked it to be on.
    let dragged = closing.clone();
    let backdrop = StyledContainer::new(
        LayoutStyle::new().flex_column().flex_grow(1.0),
        |_| RectStyle::default(),
        layers,
    )?
    .on_key(move |key: &Key| keys(key))
    .on_drag(move |_x, _y| dragged())
    .drag_button(PointerButton::Secondary);

    let overlay = Overlay::new(LayoutStyle::new().flex_column(), vec![box_item(backdrop)])?;
    hand_keyboard_in(overlay.content_node(), list, deepest);
    Ok(box_item(overlay))
}

/// Takes the keyboard into the menu as it opens, keeps it on the deepest panel open — the one the keys go to — and gives it back to whatever held it when the menu goes, however it went.
fn hand_keyboard_in(content: NodeId, list: FocusId, deepest: Deepest) {
    let handback = popup::Handback::default();
    handback.remember(content);
    focus::request(list);
    effect(move || {
        let panel = deepest();
        if focus::left_within(content) {
            focus::request(panel);
        }
    });
    on_cleanup(move || handback.give_back());
}

/// The area a panel is kept inside: the overlay's, narrowed to the box the caller gave.
fn kept_inside(within: Rect) -> Rect {
    let surface = popup::viewport();
    surface.intersect(within).unwrap_or(surface)
}

type Anchored = Rc<dyn Fn() -> Anchoring>;

/// The keys a panel answers, with whether it took each: a panel with a submenu open offers them to the submenu first.
type Keys = Rc<dyn Fn(&Key) -> bool>;

/// The list of the deepest panel open at or below one, read reactively: the one the keys go to, and so the one that should hold focus.
type Deepest = Rc<dyn Fn() -> FocusId>;

/// Each submenu a panel has built, by the index of the row it hangs off.
type Subs = Rc<RefCell<HashMap<usize, Sub>>>;

/// A submenu as the panel it opens from needs it: what it answers the keyboard with, and the deepest list open at or below it.
#[derive(Clone)]
struct Sub {
    keys: Keys,
    deepest: Deepest,
}

/// One panel of a menu, as the panel or the backdrop it opened from needs it.
struct Built {
    panel: Box<dyn LayoutItem>,
    /// The panels its submenus open, laid over the menu beside it: inside the row they hang off, they would scroll with the panel and be cut off at its edge.
    layers: Vec<Box<dyn LayoutItem>>,
    keys: Keys,
    list: FocusId,
    deepest: Deepest,
}

/// What a submenu is placed against: the panel its row is in, which exists only once every row has been built.
#[derive(Default)]
struct Host {
    panel: Cell<Option<(NodeId, RwSignal<Rect>)>>,
    view: RefCell<Option<ScrollViewport>>,
}

/// What every row of one panel shares.
#[derive(Clone)]
struct Panel {
    style: MenuStyle,
    width: f32,
    within: Rect,
    highlighted: RwSignal<Option<usize>>,
    opened: RwSignal<Option<usize>>,
    closing: Rc<Rc<dyn Fn()>>,
    host: Rc<Host>,
    subs: Subs,
    /// Bumped when a submenu is first built, which is after the row that opens it says it is open.
    built: RwSignal<u64>,
}

/// One row as its panel needs it: the widget, what picking it does, where it is for the keyboard to keep in view, and the submenu it opens.
struct Row {
    widget: Box<dyn LayoutItem>,
    act: Act,
    node: NodeId,
    control: Option<FocusId>,
    layer: Option<Box<dyn LayoutItem>>,
}

impl Built {
    /// One panel of a menu, and the panels its submenus open. Recursive: a submenu is a menu, placed beside its row rather than at a point.
    fn panel(
        entries: Vec<Entry>,
        anchoring: Anchored,
        width: f32,
        within: Rect,
        style: MenuStyle,
        closing: Rc<Rc<dyn Fn()>>,
    ) -> Result<Self, LayoutError> {
        let stops: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.stops())
            .map(|(index, _)| index)
            .collect();
        let state = Panel {
            style,
            width,
            within,
            highlighted: signal(None),
            opened: signal(None),
            closing,
            host: Rc::default(),
            subs: Rc::default(),
            built: signal(0),
        };

        let mut widgets = Vec::new();
        // The action rides on the entry, so the keyboard handler has to be told what its rows do.
        let mut acts = Vec::new();
        let mut nodes = Vec::new();
        let mut controls = Vec::new();
        let mut layers = Vec::new();
        for (index, entry) in entries.into_iter().enumerate() {
            let row = row(entry, index, &state)?;
            widgets.push(row.widget);
            acts.push(row.act);
            nodes.push(row.node);
            controls.push(row.control);
            layers.extend(row.layer);
        }

        let highlighted = state.highlighted;
        let cursor = Rc::new(move || highlighted.get().and_then(|at| nodes.get(at).copied()));
        let pointed = Rc::new(move || {
            highlighted
                .get()
                .and_then(|at| controls.get(at).copied().flatten())
        });
        let rows = Container::new(LayoutStyle::new().flex_column(), widgets)?;
        let opened = popup::popup(
            popup::Popup {
                anchoring,
                width,
                padding: style.padding,
                cursor,
                keyboard: popup::Keyboard::Held { pointed },
            },
            box_item(rows),
            move |_| RectStyle {
                border: Some(telar::Border::uniform(style.border, 1.0)),
                ..RectStyle::filled(style.background, style.radius)
            },
        )?;
        let (view, list) = (opened.view, opened.list);
        // A stroke that starts on the panel is the panel's, not one away from it.
        let panel = opened.panel.holds_stroke();
        let node = panel.layout_node();
        let laid = track_layout(node).expect("a panel is registered");
        state.host.panel.set(Some((node, laid)));
        *state.host.view.borrow_mut() = Some(view);

        let keys = keys(stops, acts, &state);
        let deepest: Deepest = {
            let (opened, built, subs) = (state.opened, state.built, state.subs.clone());
            Rc::new(move || {
                built.get();
                let sub = opened.get().and_then(|at| subs.borrow().get(&at).cloned());
                sub.map_or(list, |sub| (sub.deepest)())
            })
        };
        Ok(Self {
            panel: box_item(panel),
            layers,
            keys,
            list,
            deepest,
        })
    }
}

/// Where a submenu opens: beside the panel its row is in, level with the row, on whichever side has the room.
fn beside(row: NodeId, panel: &Panel) -> Anchored {
    let (host, padding, within) = (panel.host.clone(), panel.style.padding, panel.within);
    let laid = track_layout(row).expect("a row is registered");
    Rc::new(move || {
        laid.get();
        if let Some(view) = host.view.borrow().as_ref() {
            view.offset().1.get();
        }
        let row = telar::anchor_rect(row, &laid);
        let side = match host.panel.get() {
            Some((panel, rect)) => {
                rect.get();
                telar::anchor_rect(panel, &rect)
            }
            None => row,
        };
        // Its own padding above the row, so its first row lines up with the one it opened from.
        let anchor = Rect::new(side.x, row.y - padding, side.width, row.height);
        Anchoring::new(anchor, Placement::End, kept_inside(within)).align(AnchorAlign::Start)
    })
}

/// The keys a panel answers, and only while it is the deepest one open: a menu with a submenu showing hands the keys to it first, and takes back only the one that closes it.
fn keys(stops: Vec<usize>, acts: Vec<Act>, panel: &Panel) -> Keys {
    let Panel {
        highlighted,
        opened,
        closing,
        subs,
        ..
    } = panel.clone();
    Rc::new(move |key| {
        if let Some(open) = opened.peek() {
            let sub = subs.borrow().get(&open).map(|sub| sub.keys.clone());
            if sub.is_some_and(|sub| sub(key)) {
                return true;
            }
            if matches!(key, Key::Named(NamedKey::ArrowLeft)) {
                opened.set(None);
                return true;
            }
            return false;
        }
        let step = |by: isize| {
            if stops.is_empty() {
                return None;
            }
            let at = highlighted
                .peek()
                .and_then(|held| stops.iter().position(|stop| *stop == held));
            let next = match (at, by > 0) {
                (None, true) => 0,
                (None, false) => stops.len() - 1,
                (Some(at), _) => (at as isize + by).rem_euclid(stops.len() as isize) as usize,
            };
            Some(stops[next])
        };
        let held = || highlighted.peek().and_then(|at| acts.get(at).cloned());
        match key {
            Key::Named(NamedKey::ArrowDown) => highlighted.set(step(1)),
            Key::Named(NamedKey::ArrowUp) => highlighted.set(step(-1)),
            Key::Named(NamedKey::Home) => highlighted.set(stops.first().copied()),
            Key::Named(NamedKey::End) => highlighted.set(stops.last().copied()),
            // Rightwards into a submenu, the other half of the leftwards out of one.
            Key::Named(NamedKey::ArrowRight) => match held() {
                Some(Act::Open) => opened.set(highlighted.peek()),
                _ => return false,
            },
            Key::Named(NamedKey::Enter | NamedKey::Space) => match held() {
                Some(Act::Pick(act)) => {
                    closing();
                    act();
                }
                Some(Act::Open) => opened.set(highlighted.peek()),
                _ => {}
            },
            Key::Named(NamedKey::Escape) => closing(),
            _ => return false,
        }
        true
    })
}

/// What a row does when it is picked, as the panel's keyboard needs to know it.
#[derive(Clone)]
enum Act {
    Pick(Rc<dyn Fn()>),
    /// A submenu: picking it opens rather than commits.
    Open,
    /// A separator, a heading, a strip that answers for itself.
    None,
}

/// One row, whatever kind it is.
fn row(entry: Entry, index: usize, panel: &Panel) -> Result<Row, LayoutError> {
    let Panel {
        style,
        highlighted,
        opened,
        ..
    } = *panel;
    let closing = panel.closing.clone();
    match entry {
        Entry::Separator => {
            let rule = StyledContainer::new(
                LayoutStyle::new()
                    .width(SizeDimension::Percent(1.0))
                    .height(1.0)
                    .margin_block_start(style.padding * 0.5)
                    .margin_block_end(style.padding * 0.5),
                move |_| RectStyle::filled(style.border, 0.0),
                vec![],
            )?;
            Ok(Row {
                node: rule.layout_node(),
                widget: box_item(rule),
                act: Act::None,
                control: None,
                layer: None,
            })
        }
        Entry::Custom { widget, act } => {
            let lit = move || match highlighted.get() == Some(index) {
                true => RectStyle::filled(style.highlight, style.radius * 0.5),
                false => RectStyle::default(),
            };
            let held = act.clone();
            let stopping = match &act {
                Some(act) => Act::Pick(Rc::clone(act)),
                None => Act::None,
            };
            let custom = StyledContainer::new(
                LayoutStyle::new()
                    .flex_row()
                    .width(SizeDimension::Percent(1.0)),
                move |_| lit(),
                vec![widget()?],
            )?
            .on_hover(move |now| {
                if now && act.is_some() {
                    highlighted.set(Some(index));
                }
            })
            .on_press(move || {
                if let Some(act) = &held {
                    closing();
                    act();
                }
            });
            Ok(Row {
                node: custom.layout_node(),
                widget: box_item(custom),
                act: stopping,
                control: None,
                layer: None,
            })
        }
        Entry::Row {
            label,
            hint,
            act,
            enabled,
        } => {
            // The entry's own action and nothing else: closing is the panel's to do, once, wherever the pick came from.
            let stopping = match enabled {
                true => Act::Pick(act.clone()),
                false => Act::None,
            };
            let line = line(label, hint, index, enabled, false, panel, move || {
                closing();
                act();
            })?;
            Ok(Row {
                node: line.layout_node(),
                control: line.focus_id(),
                widget: box_item(line),
                act: stopping,
                layer: None,
            })
        }
        Entry::Sub { label, entries } => {
            let opening = line(
                label,
                style.submenu.to_string(),
                index,
                true,
                true,
                panel,
                move || opened.set(Some(index)),
            )?;
            let node = opening.layout_node();
            let anchoring = beside(node, panel);
            let (width, within, subs, built) =
                (panel.width, panel.within, panel.subs.clone(), panel.built);
            // Built the first time it is opened: a menu with six submenus is six panels nobody asked for, and the row it hangs off has no place on screen until the panel it is in has one.
            let layer = telar::Lazy::new(
                LayoutStyle::new()
                    .absolute()
                    .inset_top(0.0)
                    .inset_start(0.0),
                move || opened.get() == Some(index),
                move || {
                    let sub = Built::panel(entries, anchoring, width, within, style, closing)?;
                    subs.borrow_mut().insert(
                        index,
                        Sub {
                            keys: sub.keys,
                            deepest: sub.deepest,
                        },
                    );
                    built.update(|count| *count += 1);
                    let mut layers = vec![sub.panel];
                    layers.extend(sub.layers);
                    Ok(layers)
                },
            )?;
            Ok(Row {
                node,
                control: opening.focus_id(),
                widget: box_item(opening),
                act: Act::Open,
                layer: Some(box_item(layer)),
            })
        }
    }
}

/// A drawn row: its label, its hint, and the highlight behind them.
fn line(
    label: String,
    hint: String,
    index: usize,
    enabled: bool,
    // A submenu opens by being hovered; an ordinary row does nothing until it is pressed.
    opens: bool,
    panel: &Panel,
    act: impl Fn() + 'static,
) -> Result<StyledContainer, LayoutError> {
    let Panel {
        style,
        highlighted,
        opened,
        ..
    } = *panel;
    let ink = match enabled {
        true => style.label,
        false => style.faint,
    };
    let name = Text::declaring(
        move || label.clone(),
        LayoutStyle::new(),
        move |inherited| shared::in_family_of(TextStyle::new(style.font_size, ink), inherited),
    )?;
    let key = Text::declaring(
        move || hint.clone(),
        LayoutStyle::new(),
        move |inherited| {
            shared::in_family_of(TextStyle::new(style.font_size, style.faint), inherited)
        },
    )?;
    let row = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .width(SizeDimension::Percent(1.0))
            .height(style.row_height)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN)
            .padding_horizontal(style.padding * 1.5),
        move |_| match highlighted.get() == Some(index) && enabled {
            true => RectStyle::filled(style.highlight, style.radius * 0.5),
            false => RectStyle::default(),
        },
        vec![box_item(name), box_item(key)],
    )?
    .on_hover(move |now| {
        if !now || !enabled {
            return;
        }
        highlighted.set(Some(index));
        match opens {
            // A submenu opens by being pointed at, as every menu bar there has ever been does; one that waits for a press makes somebody click twice to reach a row they can already see the way to. The press stays: it is how a submenu opens from the keyboard, and how a pointer arriving by a straight line gets in.
            true => opened.set(Some(index)),
            // A pointer moving down a menu closes the submenu it left, which is what makes hovering back and forth across a list of them show one at a time.
            false if opened.peek() != Some(index) => opened.set(None),
            false => {}
        }
    })
    .disabled(move || !enabled)
    .presented(Role::MenuItem);
    let row = match opens {
        true => row.expanded(move || opened.get() == Some(index)),
        false => row,
    };
    Ok(match enabled {
        true => row.on_press(act).cursor(telar::Cursor::Pointer),
        false => row,
    })
}

#[cfg(test)]
#[path = "context_menu_test.rs"]
mod tests;
