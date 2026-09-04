//! The menu a gesture asks for, wherever it was asked.
//!
//! Not [`menu`](crate::menu), which is a labelled button that drops a list under itself. This one has no trigger at all: it opens at a point, because the thing it is about is what was under the pointer. That is the difference the catalogue was missing, and every application that needed one wrote the panel, the placement, the dismissal and the keyboard again — and stopped at the panel and the placement, which is why a context menu almost never answers an arrow key.
//!
//! **What it owns:** where the panel goes and that it stays on screen, which row is highlighted, the keys that move and commit, the submenu that opens beside a row, and every way of closing it. **What it does not own:** what the rows are, what they do, and what they look like — an entry carries its own action, a caller can hand over a whole widget for a row that is not a line of text, and the panel's paint is amendable.

use std::rc::Rc;
use telar_macros::Props;

use geometry_core::Rect;
use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle, SizeDimension};
use platform_core::{Key, NamedKey, PointerButton};
use reactive_core::{RwSignal, signal};
use renderer_core::{Color, RectStyle, TextStyle};
use ui_core::{Children, LayoutItem, Overlay, StyledContainer, Text, box_item};

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
    /// The height of a row the component draws itself. A `Custom` row is whatever its widget measures.
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
            row_height: 22.0,
            padding: 4.0,
            submenu: "›",
        }
    }
}

/// The list a menu's children push themselves into as they build.
///
/// A context menu's rows are heterogeneous and half of them are only there when they apply, which in markup is an `if` around a row and in Rust was a `Vec` built with pushes. The pieces below register what they are and hand back a bare node, the way a bound list's pieces do for [`ListContext::declare`](crate::list): the panel still owns the highlight, the keyboard and the submenus, because it still has the entries — they are simply written where they are read now.
#[derive(Clone)]
struct MenuEntries(Rc<std::cell::RefCell<Declared>>);

/// What a menu's children left behind: the entries they registered, and every node they handed back.
///
/// The nodes are kept because nothing frees a layout node on drop and `remove` does not reach descendants — so taking only what `take_default` returns would leak a row wrapped in anything: a group of rows written as a component of its own, an `if`, a `for`. Each piece knows its own node, so each piece says so.
#[derive(Default)]
struct Declared {
    entries: Vec<Entry>,
    nodes: Vec<layout_core::NodeId>,
}

/// Runs `children` for their entries alone, and takes what they handed back out of the tree: a piece that registered itself is not a widget.
fn declared(children: &Children) -> Result<Vec<Entry>, LayoutError> {
    let collected = MenuEntries(Rc::new(std::cell::RefCell::new(Declared::default())));
    let mut built = children.build_with(collected.clone())?;
    for item in built.take_default() {
        ui_core::remove_node(item.layout_node());
    }
    let left = collected.0.take();
    for node in left.nodes {
        ui_core::remove_node(node);
    }
    Ok(left.entries)
}

/// Registers one entry with the menu being built, and hands back the bare node every piece returns.
fn register(entry: Entry) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Some(collected) = ui_core::use_context::<MenuEntries>() else {
        // Silence here would be a row that vanished: the author wrote it, nothing drew it, and nothing said why.
        return Err(LayoutError::Engine(
            "a menu row belongs inside a `context_menu`, which is what collects it".to_string(),
        ));
    };
    let bare = StyledContainer::new(LayoutStyle::new(), |_| RectStyle::default(), vec![])?;
    let mut left = collected.0.borrow_mut();
    left.entries.push(entry);
    left.nodes.push(bare.layout_node());
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
    let panel = panel(
        entries,
        Some(at),
        left_edge(at, width, within),
        width,
        within,
        style,
        closing.clone(),
    )?;

    // A see-through sheet inside a blocking overlay, so the press never reaches whatever the menu is about. It closes on the stroke rather than the tap: a drag with no threshold reports from the press itself, so this fires the moment the button goes down, which also covers the hand that presses away and keeps moving. Waiting for a release a drag never delivers is how a menu ends up standing over a window nobody asked it to be on.
    let dragged = closing.clone();
    let backdrop = StyledContainer::new(
        LayoutStyle::new().flex_column().flex_grow(1.0),
        |_| RectStyle::default(),
        vec![panel],
    )?
    .on_drag(move |_x, _y| dragged())
    .drag_button(PointerButton::Secondary);

    Ok(box_item(Overlay::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(backdrop)],
    )?))
}

/// One panel of a menu, and the panels its submenus open. Recursive: a submenu is a menu.
///
/// `at` places the panel in surface coordinates and is what the menu itself is opened with; a submenu passes `None`, because it is positioned against the row it hangs off rather than against the window. `left` is where the panel ends up either way, which is all its children need to know to pick the side they open on.
#[allow(clippy::too_many_arguments)]
fn panel(
    entries: Vec<Entry>,
    at: Option<(f32, f32)>,
    left: f32,
    width: f32,
    within: Rect,
    style: MenuStyle,
    closing: Rc<Rc<dyn Fn()>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let stops: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.stops())
        .map(|(index, _)| index)
        .collect();
    let highlighted: RwSignal<Option<usize>> = signal(None);
    let opened: RwSignal<Option<usize>> = signal(None);

    let mut rows: Vec<Box<dyn LayoutItem>> = Vec::new();
    // The action rides on the entry, so the panel — which is what hears the key — has to be told what its rows do.
    let mut acts: Vec<Act> = Vec::new();
    for (index, entry) in entries.into_iter().enumerate() {
        let (widget, act) = row(
            entry,
            index,
            style,
            left,
            width,
            within,
            highlighted,
            opened,
            closing.clone(),
        )?;
        rows.push(widget);
        acts.push(act);
    }

    let tall = style.row_height * stops.len().max(1) as f32 + style.padding * 2.0;
    let placed = placed_at(at, width, tall, within);
    let keyed = keys(stops, acts, highlighted, opened, closing);
    Ok(box_item(
        StyledContainer::new(
            placed(),
            move |_| RectStyle {
                border: Some(renderer_core::Border::uniform(style.border, 1.0)),
                ..RectStyle::filled(style.background, style.radius)
            },
            rows,
        )?
        .styled_by(placed)
        // A press on the panel's own padding is not a press away from it, and a stroke that starts on the panel is the panel's.
        .on_press(|| {})
        .holds_stroke()
        .on_key(keyed),
    ))
}

/// Where the panel sits: at the point it was asked for, kept inside the box it was given — or wherever the row it hangs off has already put it, for a submenu.
fn placed_at(
    at: Option<(f32, f32)>,
    width: f32,
    tall: f32,
    within: Rect,
) -> impl Fn() -> LayoutStyle + Clone {
    let base = move || {
        LayoutStyle::new()
            .flex_column()
            .width(width)
            .padding_all(4.0)
    };
    let put = at.map(|(x, y)| {
        (
            x.min(within.x + within.width - width).max(within.x),
            y.min(within.y + within.height - tall).max(within.y),
        )
    });
    move || match put {
        Some((left, top)) => base().margin_from_left(left).margin_block_start(top),
        None => base(),
    }
}

/// Where a panel opened at `at` actually starts, which is what its submenus need to pick their side.
fn left_edge(at: (f32, f32), width: f32, within: Rect) -> f32 {
    at.0.min(within.x + within.width - width).max(within.x)
}

/// The keys a panel answers, and only while it is the deepest one open: a menu with a submenu showing has handed the arrows to it.
fn keys(
    stops: Vec<usize>,
    acts: Vec<Act>,
    highlighted: RwSignal<Option<usize>>,
    opened: RwSignal<Option<usize>>,
    closing: Rc<Rc<dyn Fn()>>,
) -> impl Fn(&Key) + 'static {
    move |key| {
        if opened.peek().is_some() && !matches!(key, Key::Named(NamedKey::ArrowLeft)) {
            return;
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
            Key::Named(NamedKey::ArrowLeft) if opened.peek().is_some() => opened.set(None),
            // Rightwards into a submenu, the other half of the leftwards out of one.
            Key::Named(NamedKey::ArrowRight) => {
                if let Some(Act::Open) = held() {
                    opened.set(highlighted.peek());
                }
            }
            Key::Named(NamedKey::Enter | NamedKey::Space) => match held() {
                Some(Act::Pick(act)) => {
                    closing();
                    act();
                }
                Some(Act::Open) => opened.set(highlighted.peek()),
                _ => {}
            },
            Key::Named(NamedKey::Escape) => closing(),
            _ => {}
        }
    }
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
#[allow(clippy::too_many_arguments)]
fn row(
    entry: Entry,
    index: usize,
    style: MenuStyle,
    left: f32,
    width: f32,
    within: Rect,
    highlighted: RwSignal<Option<usize>>,
    opened: RwSignal<Option<usize>>,
    closing: Rc<Rc<dyn Fn()>>,
) -> Result<(Box<dyn LayoutItem>, Act), LayoutError> {
    match entry {
        Entry::Separator => Ok((
            box_item(StyledContainer::new(
                LayoutStyle::new()
                    .width(SizeDimension::Percent(1.0))
                    .height(1.0)
                    .margin_block_start(style.padding * 0.5)
                    .margin_block_end(style.padding * 0.5),
                move |_| RectStyle::filled(style.border, 0.0),
                vec![],
            )?),
            Act::None,
        )),
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
            let done = closing.clone();
            Ok((
                box_item(
                    StyledContainer::new(
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
                            done();
                            act();
                        }
                    }),
                ),
                stopping,
            ))
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
            let done = closing.clone();
            let widget = line(
                label,
                hint,
                index,
                enabled,
                style,
                highlighted,
                opened,
                false,
                move || {
                    done();
                    act();
                },
            )?;
            Ok((widget, stopping))
        }
        Entry::Sub { label, entries } => {
            let opening = line(
                label,
                style.submenu.to_string(),
                index,
                true,
                style,
                highlighted,
                opened,
                true,
                move || opened.set(Some(index)),
            )?;
            // Positioned against the row's own box rather than the window: `inset_top(0)` is the row's top edge, which is where a submenu lines up, and it costs neither a measurement nor a rect to know.
            let leans = match left + width * 2.0 <= within.x + within.width {
                true => width,
                false => -width,
            };
            // Built the first time it is opened: a menu with six submenus is six panels nobody asked for, and the row it hangs off has no place on screen until the panel it is in has one.
            let child = ui_core::Lazy::new(
                LayoutStyle::new()
                    .absolute()
                    .inset_top(0.0)
                    .inset_start(leans),
                move || opened.get() == Some(index),
                move || {
                    Ok(vec![panel(
                        entries,
                        None,
                        left + leans,
                        width,
                        within,
                        style,
                        closing,
                    )?])
                },
            )?;
            Ok((
                box_item(StyledContainer::new(
                    LayoutStyle::new()
                        .flex_column()
                        .width(SizeDimension::Percent(1.0)),
                    |_| RectStyle::default(),
                    vec![opening, box_item(child)],
                )?),
                Act::Open,
            ))
        }
    }
}

/// A drawn row: its label, its hint, and the highlight behind them.
#[allow(clippy::too_many_arguments)]
fn line(
    label: String,
    hint: String,
    index: usize,
    enabled: bool,
    style: MenuStyle,
    highlighted: RwSignal<Option<usize>>,
    opened: RwSignal<Option<usize>>,
    // A submenu opens by being hovered; an ordinary row does nothing until it is pressed.
    opens: bool,
    act: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ink = match enabled {
        true => style.label,
        false => style.faint,
    };
    let name = Text::new(
        move || label.clone(),
        LayoutStyle::new(),
        move || TextStyle::new(style.font_size, ink),
    )?;
    let key = Text::new(
        move || hint.clone(),
        LayoutStyle::new(),
        move || TextStyle::new(style.font_size, style.faint),
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
    });
    Ok(box_item(match enabled {
        true => row.on_press(act).cursor(platform_core::Cursor::Pointer),
        false => row,
    }))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use platform_core::ModifiersState;
    use ui_core::{ComponentList, LayoutItem};

    use super::*;
    use crate::harness::{lay_out, moved, press, release, route};
    use crate::test_support::fresh_layout_runtime;

    const WINDOW: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 400.0,
        height: 300.0,
    };

    /// Every string the tree draws.
    fn drawn_texts(tree: &ComponentList) -> Vec<String> {
        tree.commands()
            .iter()
            .filter_map(|command| match command {
                renderer_core::DrawCommand::Text { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .collect()
    }

    fn key(named: NamedKey) -> platform_core::Event {
        platform_core::Event::KeyPressed {
            key: Key::Named(named),
            modifiers: ModifiersState::default(),
        }
    }

    /// A menu of three rows with a separator in the middle and a disabled one, over a shared record of what was picked and whether it asked to be closed.
    #[allow(clippy::type_complexity)]
    fn menu() -> (ComponentList, Rc<RefCell<Vec<String>>>) {
        fresh_layout_runtime();
        let said: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let entries = {
            let (one, two, three) = (said.clone(), said.clone(), said.clone());
            vec![
                Entry::row("copiar", "ctrl+c", move || {
                    one.borrow_mut().push("copiar".into())
                }),
                Entry::Separator,
                Entry::row("pegar", "ctrl+v", move || {
                    two.borrow_mut().push("pegar".into())
                })
                .disabled(),
                Entry::Sub {
                    label: "más".into(),
                    entries: vec![Entry::row("hondo", "", move || {
                        three.borrow_mut().push("hondo".into())
                    })],
                },
            ]
        };
        let closing = said.clone();
        let menu = context_menu(
            ContextMenuProps::props()
                .at((40.0, 30.0))
                .entries(entries)
                .on_close(Rc::new(move || closing.borrow_mut().push("cerrar".into())))
                .width(120.0)
                .within(WINDOW)
                .build(),
            Children::default(),
        )
        .unwrap();
        lay_out(menu.layout_node(), WINDOW.width, WINDOW.height);
        let tree = ComponentList::new(menu);
        (tree, said)
    }

    /// **The rows written where they are read.** A context menu's rows are heterogeneous and half of them are only there when they apply, which is an `if` around a row — and a `Vec` built with pushes in a function somewhere else is the one shape that cannot show that. The children register what they are, so the panel gets the same list it would have been handed.
    #[test]
    fn the_rows_may_be_children() {
        fresh_layout_runtime();
        let said: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let picked = said.clone();
        let rows = Children::new(move || {
            let picked = picked.clone();
            let mut slots = ui_core::Slots::new();
            slots.push(
                None,
                menu_row(
                    MenuRowProps::props()
                        .label("copiar")
                        .hint("ctrl+c")
                        .on_select(Rc::new(move || picked.borrow_mut().push("copiar".into())))
                        .build(),
                    Children::default(),
                )?,
            );
            slots.push(
                None,
                menu_separator(MenuSeparatorProps::props().build(), Children::default())?,
            );
            slots.push(
                None,
                menu_row(
                    MenuRowProps::props().label("pegar").disabled(true).build(),
                    Children::default(),
                )?,
            );
            Ok(slots)
        });
        let menu = context_menu(
            ContextMenuProps::props()
                .at((40.0, 30.0))
                .within(WINDOW)
                .build(),
            rows,
        )
        .unwrap();
        lay_out(menu.layout_node(), WINDOW.width, WINDOW.height);
        let mut tree = ComponentList::new(menu);

        assert!(
            drawn_texts(&tree).contains(&"copiar".to_string()),
            "the row the children declared is the row the panel drew: {:?}",
            drawn_texts(&tree)
        );

        route(&mut tree, &key(NamedKey::ArrowDown));
        route(&mut tree, &key(NamedKey::ArrowDown));
        route(&mut tree, &key(NamedKey::Enter));
        assert_eq!(
            *said.borrow(),
            vec!["copiar"],
            "a disabled row declared as a child is disabled in the panel: {:?}",
            said.borrow()
        );
    }

    /// **The arrows step over what cannot be picked.** A separator is not a row and a disabled one is there to be read, not chosen — a menu that stops on either is one where the keyboard counts lines instead of offering answers.
    #[test]
    fn the_keyboard_walks_the_rows_that_can_be_picked() {
        let (mut tree, said) = menu();

        route(&mut tree, &key(NamedKey::ArrowDown));
        route(&mut tree, &key(NamedKey::ArrowDown));
        route(&mut tree, &key(NamedKey::ArrowUp));
        route(&mut tree, &key(NamedKey::Enter));

        assert_eq!(
            *said.borrow(),
            vec!["cerrar", "copiar"],
            "{:?}",
            said.borrow()
        );
    }

    /// Rightwards into a submenu, leftwards out of it, and what is picked inside it is picked.
    #[test]
    fn a_submenu_opens_beside_its_row_and_answers_the_keyboard() {
        let (mut tree, said) = menu();
        route(&mut tree, &key(NamedKey::End));
        route(&mut tree, &key(NamedKey::ArrowRight));
        ui_core::relayout_if_dirty();

        assert!(
            drawn_texts(&tree).iter().any(|text| text == "hondo"),
            "el submenú no se abrió: {:?}",
            drawn_texts(&tree)
        );

        route(&mut tree, &key(NamedKey::ArrowDown));
        route(&mut tree, &key(NamedKey::Enter));
        assert_eq!(
            *said.borrow(),
            vec!["cerrar", "hondo"],
            "{:?}",
            said.borrow()
        );
    }

    /// **A submenu opens by being pointed at, and closes when the pointer moves off it.**
    ///
    /// The two halves are one behaviour: what makes hovering back and forth across a list of submenus show one at a time is that an ordinary row closes whatever was open. Opening on the press alone made somebody click twice to reach a row they could already see the way to.
    #[test]
    fn pointing_at_a_submenu_opens_it() {
        let (mut tree, _said) = menu();
        assert!(!drawn_texts(&tree).iter().any(|text| text == "hondo"));

        // The fourth entry down, at the metrics the default style lays it out on: two rows and a separator above it, from a panel opened at y=30.
        route(&mut tree, &moved(60.0, 94.0));
        ui_core::relayout_if_dirty();
        assert!(
            drawn_texts(&tree).iter().any(|text| text == "hondo"),
            "el submenú no se abrió al apuntarlo: {:?}",
            drawn_texts(&tree)
        );

        route(&mut tree, &moved(60.0, 45.0));
        ui_core::relayout_if_dirty();
        assert!(
            !drawn_texts(&tree).iter().any(|text| text == "hondo"),
            "salir de la fila no lo cerró: {:?}",
            drawn_texts(&tree)
        );
    }

    /// Escape is done with it, and so is a press anywhere off the panel — while a press *on* it is not.
    #[test]
    fn every_way_out_says_so_once() {
        let (mut tree, said) = menu();
        // On the row that is there to be read and not chosen: inside the panel, and not an answer.
        route(&mut tree, &press(60.0, 70.0));
        route(&mut tree, &release(60.0, 70.0));
        assert!(
            said.borrow().is_empty(),
            "una pulsación en el panel lo cerró"
        );

        route(&mut tree, &press(380.0, 280.0));
        route(&mut tree, &release(380.0, 280.0));
        assert_eq!(*said.borrow(), vec!["cerrar"], "el clic fuera no lo cerró");

        said.borrow_mut().clear();
        route(&mut tree, &key(NamedKey::Escape));
        assert_eq!(*said.borrow(), vec!["cerrar"], "escape no lo cerró");
    }
}
