//! The panel every menu opens, whichever way it was opened: a button dropping it (`menu`) or a gesture asking for it at a point (`context_menu`). One panel, so a row, a submenu, a key and a token mean the same thing in both.
//!
//! **What it owns:** the rows' look and the cursor, the keys that move and commit, the submenus that open beside a row, the overlay the panels stand in, and the keyboard going into the menu and back out of it. **What it does not own:** where the first panel is placed — its opener says that — and what the rows are, which are its children: `item`, `separator`, `group` and `submenu`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use telar::{
    AnchorAlign, Anchoring, Border, Children, Color, Container, HasPopup, Key, LayoutError,
    LayoutItem, LayoutStyle, NamedKey, NodeId, Overlay, Placement, PointerButton, Props, Reactive,
    Rect, RectStyle, RwSignal, ScrollViewport, StyledContainer, SurfaceStyle, amend_surface,
    box_item, effect,
    focus::{self, FocusId},
    memo, on_cleanup, signal, track_layout, use_context,
};

use crate::list::{self, Act, ListContext, Mark, RowLook};
use crate::menu::MenuTokens;
use crate::popup;
use crate::shared;

pub(crate) type Anchored = Rc<dyn Fn() -> Anchoring>;

/// The keys a panel answers, with whether it took each: a panel with a submenu open offers them to the submenu first.
pub(crate) type Keys = Rc<dyn Fn(&Key) -> bool>;

/// The list of the deepest panel open at or below one, read reactively: the one the keys go to, and so the one that should hold focus.
type Deepest = Rc<dyn Fn() -> FocusId>;

/// What every panel of one menu, its submenus' included, is drawn and closed with.
pub(crate) struct Config {
    /// The menu's own tokens, over the theme's.
    pub(crate) tokens: MenuTokens,
    /// Every panel's width where the caller names one; else the tokens'.
    pub(crate) width: Option<f32>,
    /// The box every panel is kept inside.
    pub(crate) within: Rect,
    /// Amends every panel's paint.
    pub(crate) style: SurfaceStyle,
    /// Tints the rows' washes. `Color::TRANSPARENT` leaves the theme's.
    pub(crate) color: Reactive<Color>,
    /// Closes the whole menu: a pick, Escape on its first panel, a press away.
    pub(crate) close: Rc<dyn Fn()>,
}

impl Config {
    /// The tokens a menu draws with: its own over the theme's, read where they are used so a theme switched while the menu is open repaints it.
    fn look(&self) -> MenuTokens {
        self.tokens.or(MenuTokens::current())
    }

    pub(crate) fn panel_width(&self) -> f32 {
        self.width.unwrap_or_else(|| self.look().width())
    }
}

/// Whether a panel is the menu's first or a submenu, which decides what Escape and the leftward arrow do in it.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Level {
    Root,
    Sub,
}

/// One panel to open: its rows, what picking one of them reports, and where it goes.
pub(crate) struct Spec {
    pub(crate) rows: Children,
    /// Fired with the index of the picked row among this panel's rows.
    pub(crate) on_select: Option<Rc<dyn Fn(u32)>>,
    pub(crate) anchoring: Anchored,
    pub(crate) width: f32,
    pub(crate) level: Level,
    /// Opens with the cursor on the first row it can take, as a menu opened from the keyboard does.
    pub(crate) seed: bool,
}

/// One open panel, as its opener or the panel it opened from needs it.
pub(crate) struct Built {
    pub(crate) panel: Box<dyn LayoutItem>,
    /// The panels its submenus open, laid over the menu beside it: inside the row they hang off, they would scroll with the panel and be cut off at its edge.
    pub(crate) layers: Vec<Box<dyn LayoutItem>>,
    pub(crate) keys: Keys,
    pub(crate) list: FocusId,
    deepest: Deepest,
    /// Puts the cursor on the first row, or on none, as the panel is shown again.
    enter: Rc<dyn Fn(bool)>,
}

/// The submenus hanging off one panel's rows, and what those rows reach the panel through.
pub(crate) struct Cascade {
    config: Rc<Config>,
    opened: RwSignal<Option<u32>>,
    subs: RefCell<HashMap<u32, Sub>>,
    /// Bumped when a submenu is first built, which is after the row that opens it says it is open.
    built: RwSignal<u64>,
    host: Host,
    layers: RefCell<Vec<Box<dyn LayoutItem>>>,
    /// Set as a submenu not yet built opens from the keyboard, so it is built with the cursor on its first row.
    seeding: Cell<bool>,
}

/// A submenu as the panel it opens from needs it.
#[derive(Clone)]
struct Sub {
    keys: Keys,
    deepest: Deepest,
    enter: Rc<dyn Fn(bool)>,
}

/// What a submenu is placed against: the panel its row is in, which exists only once every row has been built.
#[derive(Default)]
struct Host {
    panel: Cell<Option<(NodeId, RwSignal<Rect>)>>,
    view: RefCell<Option<ScrollViewport>>,
}

impl Cascade {
    fn new(config: Rc<Config>) -> Self {
        Self {
            config,
            opened: signal(None),
            subs: RefCell::default(),
            built: signal(0),
            host: Host::default(),
            layers: RefCell::default(),
            seeding: Cell::new(false),
        }
    }

    /// How the panel's rows are drawn: the menu's tokens.
    pub(crate) fn look(&self) -> RowLook {
        let look = self.config.look();
        RowLook {
            height: look.row_height(),
            label: look.label,
            faint: Some(look.faint()),
            highlight: look.highlight,
        }
    }

    /// The pointer came to rest on row `index`: a submenu's row opens it, as every menu bar there has ever been does, and any other row closes the one that was open, which is what makes hovering back and forth across a list of them show one at a time.
    pub(crate) fn pointed(&self, index: u32, opens: bool) {
        match opens {
            true => self.open(index, false),
            false if self.opened.peek().is_some_and(|open| open != index) => self.opened.set(None),
            false => {}
        }
    }

    /// Opens the submenu hanging off row `index`, with the cursor on its first row when the keyboard opened it.
    fn open(&self, index: u32, seed: bool) {
        let shown = self.opened.peek() == Some(index);
        if seed || !shown {
            let sub = self.subs.borrow().get(&index).cloned();
            match sub {
                Some(sub) => (sub.enter)(seed),
                None => self.seeding.set(seed),
            }
        }
        self.opened.set_if_changed(Some(index));
    }

    fn is_open(&self, index: u32) -> bool {
        self.opened.get() == Some(index)
    }

    /// The list of the deepest submenu open below this panel, if one is.
    fn deepest(&self) -> Option<FocusId> {
        self.built.get();
        let open = self.opened.get()?;
        let sub = self.subs.borrow().get(&open).cloned()?;
        Some((sub.deepest)())
    }

    /// Offers `key` to the submenu open below this panel. `None` while none is; else whether it was taken, the leftward arrow and Escape closing the submenu when nothing deeper took them.
    fn route(&self, key: &Key) -> Option<bool> {
        let open = self.opened.peek()?;
        let keys = self.subs.borrow().get(&open).map(|sub| sub.keys.clone());
        if keys.is_some_and(|keys| keys(key)) {
            return Some(true);
        }
        let closes = matches!(key, Key::Named(NamedKey::ArrowLeft | NamedKey::Escape));
        if closes {
            self.opened.set(None);
        }
        Some(closes)
    }
}

/// Builds one panel of a menu and everything its rows need from it. Recursive: a submenu is a panel built by its row, placed beside it.
pub(crate) fn panel(spec: Spec, config: Rc<Config>) -> Result<Built, LayoutError> {
    let Spec {
        rows,
        on_select,
        anchoring,
        width,
        level,
        seed,
    } = spec;
    let cascade = Rc::new(Cascade::new(config.clone()));
    let highlighted: RwSignal<Option<u32>> = signal(None);
    let pick: Rc<dyn Fn(u32)> = {
        let close = config.close.clone();
        Rc::new(move |index| {
            close();
            if let Some(on_select) = &on_select {
                on_select(index);
            }
        })
    };
    let list = ListContext::menu(pick, highlighted, config.color.clone(), cascade.clone());
    if seed {
        list.seed_cursor();
    }
    let built_rows = rows.build_with(list.clone())?.take_default();
    let layers = cascade.layers.take();
    let column = Container::new(LayoutStyle::new().flex_column(), built_rows)?;

    let cursor = {
        let list = list.clone();
        Rc::new(move || highlighted.get().and_then(|at| list.node_of(at)))
    };
    let pointed = {
        let list = list.clone();
        Rc::new(move || highlighted.get().and_then(|at| list.focus_of(at)))
    };
    let paint = {
        let config = config.clone();
        move |_| {
            let look = config.look();
            let panel = RectStyle::filled(look.background(), look.radius())
                .with_border(Border::uniform(look.border(), shared::border_width()));
            amend_surface(panel, &config.style)
        }
    };
    let opened = popup::popup(
        popup::Popup {
            anchoring,
            width: popup::Width::AtLeast(width),
            padding: config.look().padding(),
            cursor,
            keyboard: popup::Keyboard::Held { pointed },
        },
        box_item(column),
        paint,
    )?;
    // A stroke that starts on the panel is the panel's, not one away from it.
    let panel = opened.panel.holds_stroke();
    let node = panel.layout_node();
    let laid = track_layout(node).expect("a panel is registered");
    cascade.host.panel.set(Some((node, laid)));
    *cascade.host.view.borrow_mut() = Some(opened.view);

    let list_id = opened.list;
    let deepest: Deepest = {
        let cascade = cascade.clone();
        Rc::new(move || cascade.deepest().unwrap_or(list_id))
    };
    let enter: Rc<dyn Fn(bool)> = {
        let list = list.clone();
        Rc::new(move |seed| highlighted.set(if seed { list.edge(1) } else { None }))
    };
    Ok(Built {
        panel: box_item(panel),
        layers,
        keys: keys(list, cascade, level),
        list: list_id,
        deepest,
        enter,
    })
}

/// The keys a panel answers, and only while it is the deepest one open: a panel with a submenu showing hands the keys to it first, and takes back only the ones that close it.
fn keys(list: ListContext, cascade: Rc<Cascade>, level: Level) -> Keys {
    Rc::new(move |key| {
        if let Some(taken) = cascade.route(key) {
            return taken;
        }
        let highlighted = list.highlighted();
        let held = highlighted.peek();
        let act = held.and_then(|at| list.act_of(at));
        let mods = telar::modifiers();
        match key {
            Key::Named(NamedKey::ArrowDown) => highlighted.set(list.step(held, 1)),
            Key::Named(NamedKey::ArrowUp) => highlighted.set(list.step(held, -1)),
            Key::Named(NamedKey::Home) => highlighted.set(list.edge(1)),
            Key::Named(NamedKey::End) => highlighted.set(list.edge(-1)),
            // A space is a character only inside a query already under way.
            Key::Named(NamedKey::Space) if list.is_searching() => {
                if let Some(at) = list.type_ahead(' ', held) {
                    highlighted.set(Some(at));
                }
            }
            Key::Named(NamedKey::ArrowRight) => match (held, act) {
                (Some(at), Some(Act::Opens)) => cascade.open(at, true),
                _ => return false,
            },
            Key::Named(NamedKey::Enter | NamedKey::Space) => match (held, act) {
                (Some(at), Some(Act::Opens)) => cascade.open(at, true),
                (Some(at), Some(Act::Run(_))) => list.commit(at),
                _ => {}
            },
            Key::Named(NamedKey::Escape) if level == Level::Root => (cascade.config.close)(),
            // Ctrl or Meta makes a character a command rather than a query. Alt is left out: on many layouts it is how a character is typed.
            Key::Char(c) if !c.is_control() && !mods.is_ctrl && !mods.is_meta => {
                if let Some(at) = list.type_ahead(*c, held) {
                    highlighted.set(Some(at));
                }
            }
            _ => return false,
        }
        true
    })
}

/// Where a submenu opens: beside the panel its row is in, level with the row, on whichever side has the room. Flush against that panel, so the pointer crosses no gap that belongs to neither on its way in.
fn beside(row: NodeId, cascade: &Rc<Cascade>) -> Anchored {
    let cascade = cascade.clone();
    let laid = track_layout(row).expect("a row is registered");
    let config = cascade.config.clone();
    let padding = memo(move || config.look().padding());
    Rc::new(move || {
        laid.get();
        let padding = padding.get();
        if let Some(view) = cascade.host.view.borrow().as_ref() {
            view.offset().1.get();
        }
        let row = telar::anchor_rect(row, &laid);
        let side = match cascade.host.panel.get() {
            Some((panel, rect)) => {
                rect.get();
                telar::anchor_rect(panel, &rect)
            }
            None => row,
        };
        // Its own padding above the row, so its first row lines up with the one it opened from.
        let anchor = Rect::new(side.x, row.y - padding, side.width, row.height);
        Anchoring::new(anchor, Placement::End, kept_inside(cascade.config.within))
            .align(AnchorAlign::Start)
    })
}

/// The area a panel is kept inside: the overlay's, narrowed to the box the caller gave.
pub(crate) fn kept_inside(within: Rect) -> Rect {
    let surface = popup::viewport();
    surface.intersect(within).unwrap_or(surface)
}

/// A menu's panels in a blocking overlay of their own, with the keyboard taken into them; `keys`, when the opener does not answer them itself, are answered on the overlay.
///
/// A see-through sheet under the panels, so a press never reaches whatever the menu is about. It closes on the stroke rather than the tap: a drag with no threshold reports from the press itself, so this fires the moment the button goes down, which also covers the hand that presses away and keeps moving. Waiting for a release a drag never delivers is how a menu ends up standing over a window nobody asked it to be on.
pub(crate) fn overlay(
    built: Built,
    close: Rc<dyn Fn()>,
    keys: Option<Keys>,
) -> Result<Overlay, LayoutError> {
    let Built {
        panel,
        layers: subs,
        list,
        deepest,
        ..
    } = built;
    let mut layers = vec![panel];
    layers.extend(subs);
    let backdrop = StyledContainer::new(
        LayoutStyle::new().flex_column().flex_grow(1.0),
        |_| RectStyle::default(),
        layers,
    )?
    .on_drag(move |_x, _y| close())
    .drag_button(PointerButton::Secondary);
    let backdrop = match keys {
        Some(keys) => backdrop.on_key(move |key: &Key| keys(key)),
        None => backdrop,
    };
    let overlay = Overlay::new(LayoutStyle::new().flex_column(), vec![box_item(backdrop)])?;
    hand_keyboard_in(overlay.content_node(), list, deepest);
    Ok(overlay)
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

#[derive(Props)]
/// A row of a menu that opens another menu beside it, whose rows are this one's children.
pub struct SubmenuProps {
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Quiet text before the mark that says it opens: the value its rows choose between, as it stands.
    #[props(some, into, default)]
    pub hint: Option<Reactive<String>>,
    /// Greys the row and keeps it shut: there, and unavailable.
    #[props(into, default)]
    pub disabled: Reactive<bool>,
    /// Fired with the index of the row picked in the submenu, among its own rows.
    #[props(some, default)]
    pub on_select: Option<Rc<dyn Fn(u32)>>,
}

/// A row that opens another menu beside it: pointed at, pressed, or reached with the rightward arrow, Enter or Space; and shut again with the leftward arrow or Escape. Belongs inside a `menu` or a `context_menu`.
pub fn submenu(
    props: SubmenuProps,
    children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let SubmenuProps {
        label,
        hint,
        disabled,
        on_select,
    } = props;
    let list = use_context::<ListContext>();
    let Some(cascade) = list.as_ref().and_then(|ctx| ctx.cascade().cloned()) else {
        // Silence here would be a row that vanished: the author wrote it, nothing drew it, and nothing said why.
        return Err(LayoutError::Engine(
            "a `submenu` belongs inside a `menu` or a `context_menu`, which is what opens it"
                .to_string(),
        ));
    };
    let ctx = list.clone().expect("a cascade comes from a list");
    let index = {
        let disabled = disabled.clone();
        ctx.claim(Reactive::of(move || !disabled.get()), label.clone())
    };
    if let Some(bare) = list::declared_placeholder(&list) {
        return bare;
    }

    let mut content = vec![
        list::tick_column(&list, None, false)?,
        box_item(list::label_text(label, disabled.clone(), &list)?),
    ];
    if let Some(hint) = hint {
        content.push(box_item(list::hint_text(hint, &list)?));
    }
    let faint = cascade.clone();
    content.push(shared::mark(
        || shared::Mark::ChevronForward,
        move |_| faint.look().faint.unwrap_or_else(shared::muted).into(),
    )?);
    let opening = cascade.clone();
    let row = list::styled_row(&list, Some(index), disabled, content)?
        .on_press(move || opening.open(index, false));
    let row = ctx.present(row, index, Mark::None);
    let shown = cascade.clone();
    let row = row
        .expanded(move || shown.is_open(index))
        .has_popup(HasPopup::Menu);
    if let Some(focus) = row.focus_id() {
        ctx.attach(index, row.layout_node(), focus, Act::Opens);
    }

    let anchoring = beside(row.layout_node(), &cascade);
    let (parent, opened) = (cascade.clone(), cascade.opened);
    // Built the first time it is opened: a menu with six submenus is six panels nobody asked for, and the row it hangs off has no place on screen until the panel it is in has one.
    let layer = telar::Lazy::new(
        LayoutStyle::new()
            .absolute()
            .inset_top(0.0)
            .inset_start(0.0),
        move || opened.get() == Some(index),
        move || {
            let config = parent.config.clone();
            let sub = panel(
                Spec {
                    rows: children,
                    on_select,
                    anchoring,
                    width: config.panel_width(),
                    level: Level::Sub,
                    seed: parent.seeding.take(),
                },
                config,
            )?;
            parent.subs.borrow_mut().insert(
                index,
                Sub {
                    keys: sub.keys,
                    deepest: sub.deepest,
                    enter: sub.enter,
                },
            );
            parent.built.update(|count| *count += 1);
            let mut layers = vec![sub.panel];
            layers.extend(sub.layers);
            Ok(layers)
        },
    )?;
    cascade.layers.borrow_mut().push(box_item(layer));
    Ok(box_item(row))
}

#[cfg(test)]
#[path = "menu_panel_test.rs"]
mod tests;
