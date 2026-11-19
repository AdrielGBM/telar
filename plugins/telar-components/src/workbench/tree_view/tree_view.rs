//! [`tree_view`]: a hierarchy of rows, opened, closed and walked from the keyboard, with only the rows on screen built.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;

use telar::{
    Accessible, AlignItems, Border, BorderRadius, Clip, ClippedItem, Color, Container, Direction,
    DropPlace, FocusRing, Handler, JustifyContent, Key, LayoutError, LayoutItem, LayoutScrollArea,
    LayoutStyle, Memo, NamedKey, PointerButton, Props, Reactive, Rect, RectStyle, RwSignal,
    ShapeStyle, StyledContainer, Text, TextWrap, Transform, box_item, current_direction,
    focus::{self, FocusId, Role, SetPosition},
    memo, signal, track_layout,
};

use super::tree_drag::{Dragging, TreeDrag, TreeRows};
use crate::overlays::menu::menu::{MenuPlace, focused_rect, is_context_menu_key};
use crate::shared;
use crate::shared::rows::{BuiltRows, RowKey, RowPlaces, RowState, cursor_ring, row_wash};
use crate::shared::type_ahead::TypeAhead;
use telar::ControlSize;

const OVERSCAN: usize = 4;

/// A tree view's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the tree is built or painted, so a tree's rows stand at the height every other list's do.
#[derive(Clone, Copy, Debug, Default, PartialEq, telar::ComponentTokens)]
pub struct TreeViewTokens {
    /// Height of a row at the regular size, scaled with the tree's own. `None` is [`metrics::row_height`](crate::metrics::row_height).
    pub row_height: Option<f32>,
    /// How far each level is set in from the one above it, likewise. `None` is [`metrics::gap_xl`](crate::metrics::gap_xl).
    #[token(or = shared::gap_xl, scaled)]
    pub indent: Option<f32>,
    /// Corner of a row's wash and its cursor. `None` is the theme's `radius_item`.
    #[token(or = shared::radius_item)]
    pub radius: Option<f32>,
    /// Space between a row's edges and what it shows, likewise. `None` is [`metrics::gap_md`](crate::metrics::gap_md).
    #[token(or = shared::gap_md, scaled)]
    pub pad_x: Option<f32>,
    /// Space between the tree's sides and its rows, so a row's wash stands clear of them as a menu's does of its panel, likewise. `None` is [`metrics::panel_pad`](crate::metrics::panel_pad).
    #[token(or = shared::panel_pad, scaled)]
    pub inset: Option<f32>,
}

impl TreeViewTokens {
    /// The height of a row of a tree of `size`, in whole pixels: a virtual list places the rows it has not built by multiplying it, and a fractional height drifts off the pixels layout snaps the built ones to.
    pub fn row_height(&self, size: ControlSize) -> f32 {
        crate::layout::list::list::scaled_row_height(self.row_height, size)
    }
}

/// One node of a tree, as data: the key the selection, the open branches and a drop name it by, what it is called, and the nodes under it.
///
/// The children sit behind an `Arc`, so handing a whole tree to a signal or cloning one row out of it copies pointers rather than the hierarchy.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TreeNode<K> {
    /// Unique across the whole tree.
    pub id: K,
    pub label: Arc<str>,
    /// Quiet trailing text: a count, a status.
    pub badge: Option<Arc<str>>,
    /// The nodes under it, or `None` for a leaf. A branch with nothing under it yet, an empty folder, holds an empty list: it still opens, and still takes what is dropped into it.
    pub children: Option<Arc<[TreeNode<K>]>>,
}

impl<K: RowKey> TreeNode<K> {
    /// A leaf.
    pub fn new(id: K, label: impl Into<Arc<str>>) -> Self {
        Self {
            id,
            label: label.into(),
            badge: None,
            children: None,
        }
    }

    pub fn with_badge(mut self, badge: impl Into<Arc<str>>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    /// A branch holding `children`, which may be none.
    pub fn with_children(mut self, children: impl IntoIterator<Item = TreeNode<K>>) -> Self {
        self.children = Some(children.into_iter().collect());
        self
    }

    /// Whether the node is a branch, something to open and to drop into, whether or not anything is under it yet.
    pub fn is_branch(&self) -> bool {
        self.children.is_some()
    }

    /// The nodes under it: none for a leaf, or for a branch with nothing in it yet.
    pub fn child_nodes(&self) -> &[TreeNode<K>] {
        self.children.as_deref().unwrap_or_default()
    }

    /// The key of every branch among `nodes` and under them, at any depth, parents before what they hold: what opening them all opens.
    pub fn branch_ids(nodes: &[TreeNode<K>]) -> Vec<K> {
        let mut ids = Vec::new();
        collect_branch_ids(nodes, &mut ids);
        ids
    }
}

fn collect_branch_ids<K: RowKey>(nodes: &[TreeNode<K>], ids: &mut Vec<K>) {
    for node in nodes.iter().filter(|node| node.is_branch()) {
        ids.push(node.id.clone());
        collect_branch_ids(node.child_nodes(), ids);
    }
}

/// A tree of rows given as data, walked with the arrow keys, Home, End, Page Up, Page Down and type-ahead, and bound to a selection and a set of open branches, each named by the caller's own key `K`.
///
/// It fills the height its parent gives it and scrolls within it, building only the rows on screen, so a tree of thousands of nodes costs what a screenful does. Every row stands at [`TreeViewTokens::row_height`] of the tree's size, unless [`wrap`](Self::wrap) lets rows grow: then each is measured once built, and stands at the row height until then.
#[derive(Props)]
pub struct TreeViewProps<K: RowKey> {
    #[props(into, default)]
    pub items: Reactive<Vec<TreeNode<K>>>,
    /// How large the rows are drawn: their height, padding, indent, caret and text, as [`ControlSize`] works them out. What [`leading`](Self::leading) and [`trailing`](Self::trailing) build is sized where it is written.
    #[props(default)]
    pub size: ControlSize,
    /// The bound selection, by node key. `None` (the default) is uncontrolled.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<Option<K>>>,
    /// The bound set of open branches, by node key. `None` (the default) is uncontrolled and starts with every branch shut. Shutting a branch shuts every branch beneath it too.
    #[props(some, into, default)]
    pub expanded: Option<RwSignal<HashSet<K>>>,
    /// Fired with the node a click, Enter or Space chose.
    #[props(some, default)]
    pub on_select: Option<Handler<fn(TreeNode<K>)>>,
    /// Fired when a row's context menu is asked for — a secondary press on the row, or the context-menu key or Shift+F10 on the row the keyboard is on — with the row and where the menu opens. The row takes the keyboard cursor first, so it reads as the one the menu is about. `None` (the default) leaves a secondary press to what is behind the tree.
    #[props(some, default)]
    pub on_context: Option<Handler<fn(TreeContext<K>)>>,
    /// Asked first about every key the tree hears, with the node the keyboard is on, and answering whether it took the key: the app's own commands, such as F2, Delete or Alt+↑, ahead of the tree's. The tree walks only on bare arrows, Home, End, Page Up and Page Down, so a combination with a modifier held is never the tree's.
    #[props(some, default)]
    pub on_key: Option<TreeKeyHandler<K>>,
    /// Whether a branch can be selected. Off, a click, Enter or Space on a branch opens or shuts it instead, for a tree whose selection only ever names a leaf. On by default.
    #[props(default = true)]
    pub select_branches: bool,
    /// Builds what a row shows before its label: an icon.
    #[props(some, default)]
    pub leading: Option<TreeRowBuilder<K>>,
    /// Builds what a row shows after its badge: a status dot, an action.
    #[props(some, default)]
    pub trailing: Option<TreeRowBuilder<K>>,
    /// How the rows take part in a [`DragSession`](telar::DragSession): carried between siblings and into branches, and taking payloads from any other widget of the session. `None` (the default) drags nothing and takes nothing.
    #[props(some, default)]
    pub drag: Option<TreeDrag<K>>,
    /// Whether a long label runs onto more lines instead of being cut off where its row ends, the row growing to hold it. Off (the default), every row is one row height tall, and nothing is measured.
    #[props(default)]
    pub wrap: bool,
    /// What a reader calls the tree.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
    /// Accent for the selection and the keyboard cursor. `None` (the default) falls back to the theme accent.
    #[props(some, into, default)]
    pub color: Option<Reactive<Color>>,
}

/// What a row's context menu was asked for on, and where it opens, as [`ContextMenuProps`](crate::ContextMenuProps) takes it: at the pointer for a secondary press, or under the row for one asked for from the keyboard.
#[derive(Clone, Debug)]
pub struct TreeContext<K> {
    pub node: TreeNode<K>,
    pub at: (f32, f32),
    pub around: Option<Rect>,
}

impl<K> TreeContext<K> {
    pub fn place(&self) -> MenuPlace {
        match self.around {
            Some(under) => MenuPlace::Under(under),
            None => MenuPlace::At(self.at),
        }
    }
}

/// What [`TreeViewProps::leading`] and [`TreeViewProps::trailing`] build a row's content with.
pub type TreeRowBuilder<K> = Rc<dyn Fn(&TreeNode<K>) -> Result<Box<dyn LayoutItem>, LayoutError>>;

/// What [`TreeViewProps::on_key`] is: told a key and the node the keyboard is on, if any, and answering whether it took the key.
pub type TreeKeyHandler<K> = Rc<dyn Fn(&Key, Option<&TreeNode<K>>) -> bool>;

/// A tree of rows given as data, with expand and collapse, keyboard navigation, type-ahead, a bound selection and, given a [`TreeDrag`], rows carried and payloads taken.
pub fn tree_view<K: RowKey>(props: TreeViewProps<K>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let TreeViewProps {
        items,
        size,
        selected,
        expanded,
        on_select,
        on_context,
        on_key,
        select_branches,
        leading,
        trailing,
        drag,
        wrap,
        label,
        color,
    } = props;
    let selected = selected.unwrap_or_else(|| signal(None));
    let expanded = expanded.unwrap_or_else(|| signal(HashSet::new()));
    let flat = memo(move || {
        let items = items.get();
        expanded.with(|open| Flat::of(&items, open))
    });
    let row_height = TreeViewTokens::current().row_height(size);
    let ring_color = color.clone();

    let mut made: Option<Rc<Tree<K>>> = None;
    let scroll = LayoutScrollArea::new_with(fill(), |viewport| {
        let places = RowPlaces::new(viewport, size, row_height, wrap);
        let drag = drag.map(|drag| {
            drag.bind(TreeRows {
                flat,
                expanded,
                places: places.clone(),
            })
        });
        let tree = Rc::new(Tree {
            flat,
            cursor: signal(None),
            selected,
            expanded,
            search: RefCell::default(),
            on_select,
            on_context,
            on_key,
            select_branches,
            places,
            focus: Cell::new(None),
            built: BuiltRows::new(),
            drag,
        });
        made = Some(tree.clone());
        let source = {
            let tree = tree.clone();
            move || tree.flat.with(|flat| flat.0.as_ref().clone())
        };
        let slots = RowSlots { leading, trailing };
        let builds = tree.clone();
        let rows = tree.places.clone().list(
            OVERSCAN,
            source,
            |row: &Rc<Row<K>>| {
                (
                    row.node.id.clone(),
                    row.node.label.clone(),
                    row.node.badge.clone(),
                    row.node.is_branch(),
                    row.position,
                )
            },
            move |_, row| build_row(&builds, row, &slots, &color),
        )?;
        let list = rows.layout_node();
        let mut content = vec![box_item(rows)];
        if let Some(drag) = &tree.drag {
            Rc::clone(drag).register(list);
            content.push(drop_line(&tree, list)?);
            content.push(Rc::clone(drag).label()?);
        }
        let inset = Container::new(inset_box(size), content)?.styled_by(move || inset_box(size));
        Ok(box_item(inset))
    })?;
    let tree = made.expect("a scroll area builds its content before it returns");

    let seeding = tree.clone();
    let keys = tree.clone();
    let cursor = tree.clone();
    let ringed = tree.clone();
    let scroll = scroll
        .control(Role::Tree)
        .focus_ring(move || ringed.own_ring(&ring_color))
        .on_focus(move |now| {
            if now {
                seeding.seed_cursor();
            }
        })
        .active_descendant(move || cursor.cursor_row())
        .on_focused_key(move |key: &Key| keys.on_key(key));
    tree.focus.set(scroll.focus_id());
    Ok(match label {
        Some(label) => box_item(scroll.a11y_label(move || label.get())),
        None => box_item(scroll),
    })
}

fn fill() -> LayoutStyle {
    LayoutStyle::new().flex_grow(1.0)
}

/// Only across: the virtual list places its rows from the content's top, so space above them would stand every row off where it reckons it is.
fn inset_box(size: ControlSize) -> LayoutStyle {
    LayoutStyle::column().padding_horizontal(TreeViewTokens::current().inset(size))
}

/// One row the open branches show.
pub(super) struct Row<K> {
    pub(super) node: TreeNode<K>,
    pub(super) position: SetPosition,
    /// The row of the branch it is under, `None` at the top level.
    pub(super) parent: Option<usize>,
}

/// The rows the open branches show, in order. Equal only to itself, so the memo notifies on every rebuild instead of comparing whole trees.
pub(super) struct Flat<K>(pub(super) Rc<Vec<Rc<Row<K>>>>);

impl<K> Clone for Flat<K> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<K> PartialEq for Flat<K> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<K: RowKey> Flat<K> {
    pub(super) fn of(items: &[TreeNode<K>], open: &HashSet<K>) -> Self {
        let mut rows = Vec::new();
        push_level(&mut rows, items, open, NonZeroU32::MIN, None);
        Self(Rc::new(rows))
    }
}

fn push_level<K: RowKey>(
    rows: &mut Vec<Rc<Row<K>>>,
    nodes: &[TreeNode<K>],
    open: &HashSet<K>,
    level: NonZeroU32,
    parent: Option<usize>,
) {
    let Some(size) = NonZeroU32::new(nodes.len() as u32) else {
        return;
    };
    for (i, node) in nodes.iter().enumerate() {
        let at = rows.len();
        rows.push(Rc::new(Row {
            node: node.clone(),
            position: SetPosition {
                level,
                position: NonZeroU32::MIN.saturating_add(i as u32),
                size,
            },
            parent,
        }));
        if node.is_branch() && open.contains(&node.id) {
            push_level(
                rows,
                node.child_nodes(),
                open,
                level.saturating_add(1),
                Some(at),
            );
        }
    }
}

/// What builds a row's content on either side of its label.
struct RowSlots<K> {
    leading: Option<TreeRowBuilder<K>>,
    trailing: Option<TreeRowBuilder<K>>,
}

struct Tree<K: RowKey> {
    flat: Memo<Flat<K>>,
    /// The row the keyboard is on. Deliberately not the selection: walking the tree commits nothing.
    cursor: RwSignal<Option<K>>,
    selected: RwSignal<Option<K>>,
    expanded: RwSignal<HashSet<K>>,
    search: RefCell<TypeAhead>,
    on_select: Option<Handler<fn(TreeNode<K>)>>,
    on_context: Option<Handler<fn(TreeContext<K>)>>,
    on_key: Option<TreeKeyHandler<K>>,
    select_branches: bool,
    places: RowPlaces,
    focus: Cell<Option<FocusId>>,
    built: BuiltRows<K>,
    drag: Option<Rc<dyn Dragging<K>>>,
}

impl<K: RowKey> Tree<K> {
    fn rows(&self) -> Rc<Vec<Rc<Row<K>>>> {
        self.flat.with(|flat| flat.0.clone())
    }

    fn index_of(rows: &[Rc<Row<K>>], id: Option<K>) -> Option<usize> {
        let id = id?;
        rows.iter().position(|row| row.node.id == id)
    }

    /// Where the keys act from: the cursor while its row is showing, else the selection's row.
    fn at(&self, rows: &[Rc<Row<K>>]) -> Option<usize> {
        Self::index_of(rows, self.cursor.peek())
            .or_else(|| Self::index_of(rows, self.selected.peek()))
    }

    fn is_open(&self, id: &K) -> bool {
        self.expanded.with(|open| open.contains(id))
    }

    fn is_selected(&self, id: &K) -> bool {
        self.selected.with(|selected| selected.as_ref() == Some(id))
    }

    fn shows_cursor(&self, id: &K) -> bool {
        self.focus.get().is_some_and(focus::is_focus_visible)
            && self.cursor.with(|cursor| cursor.as_ref() == Some(id))
    }

    fn cursor_row(&self) -> Option<FocusId> {
        self.built.get(&self.cursor.get()?)
    }

    /// Whether a payload let go now would land onto the row `id`.
    fn takes_drop_onto(&self, id: &K) -> bool {
        let Some(DropPlace::Inside(index)) = self.drag.as_ref().and_then(|drag| drag.place()) else {
            return false;
        };
        self.flat
            .with(|flat| flat.0.get(index).is_some_and(|row| row.node.id == *id))
    }

    /// The ring the tree wears itself while it holds the keyboard: the cursor's, while the cursor rests on no row showing, so an empty tree still shows where the keyboard is.
    fn own_ring(&self, color: &Option<Reactive<Color>>) -> Option<FocusRing> {
        Self::index_of(&self.rows(), self.cursor.get())
            .is_none()
            .then(|| cursor_ring(color))
    }

    /// Puts the cursor where Tab arriving at the tree should find it: on the selection, or on the first row.
    fn seed_cursor(&self) {
        let rows = self.rows();
        if Self::index_of(&rows, self.cursor.peek()).is_some() {
            return;
        }
        if let Some(i) =
            Self::index_of(&rows, self.selected.peek()).or((!rows.is_empty()).then_some(0))
        {
            self.cursor.set(Some(rows[i].node.id.clone()));
        }
    }

    fn move_to(&self, rows: &[Rc<Row<K>>], index: usize) {
        self.cursor.set(Some(rows[index].node.id.clone()));
        self.reveal(index);
    }

    /// Puts the cursor on the row `id` and the keyboard on the tree, as a reader moving to that row asks.
    fn reach(&self, id: &K) {
        let rows = self.rows();
        if let Some(index) = rows.iter().position(|row| row.node.id == *id) {
            self.move_to(&rows, index);
        }
        if let Some(tree) = self.focus.get() {
            focus::request(tree);
        }
    }

    fn reveal(&self, index: usize) {
        self.places.reveal(index);
    }

    /// What a click, Enter or Space does to the row at `index`: selects it, or opens or shuts it when it is a branch the tree does not select.
    fn activate(&self, rows: &[Rc<Row<K>>], index: usize) {
        let row = &rows[index];
        if row.node.is_branch() && !self.select_branches {
            self.cursor.set(Some(row.node.id.clone()));
            self.toggle(rows, index);
        } else {
            self.select(row);
        }
    }

    fn activate_id(&self, id: &K) {
        let rows = self.rows();
        if let Some(index) = rows.iter().position(|row| row.node.id == *id) {
            self.activate(&rows, index);
        }
    }

    fn select(&self, row: &Row<K>) {
        let id = row.node.id.clone();
        self.cursor.set(Some(id.clone()));
        self.selected.set(Some(id));
        if let Some(on_select) = &self.on_select {
            on_select.run(row.node.clone());
        }
    }

    fn open(&self, id: &K) {
        self.expanded.update(|set| {
            set.insert(id.clone());
        });
    }

    /// Shuts `node` and every branch beneath it, so opening it again shows its children shut.
    fn shut(&self, node: &TreeNode<K>) {
        let within = TreeNode::branch_ids(std::slice::from_ref(node));
        self.expanded
            .update(|set| set.retain(|id| !within.contains(id)));
    }

    /// Opens or shuts the branch at `index`; shutting one with the cursor inside moves the cursor onto it, where it would otherwise vanish.
    fn toggle(&self, rows: &[Rc<Row<K>>], index: usize) {
        let row = &rows[index];
        if !self.is_open(&row.node.id) {
            self.open(&row.node.id);
            return;
        }
        let cursor = self.cursor.peek();
        let holds_cursor = rows[index + 1..]
            .iter()
            .take_while(|inner| inner.position.level > row.position.level)
            .any(|inner| Some(&inner.node.id) == cursor.as_ref());
        if holds_cursor {
            self.cursor.set(Some(row.node.id.clone()));
        }
        self.shut(&row.node);
    }

    fn toggle_id(&self, id: &K) {
        let rows = self.rows();
        if let Some(index) = rows.iter().position(|row| row.node.id == *id) {
            self.toggle(&rows, index);
        }
    }

    fn ask_context(&self, row: &Row<K>, at: (f32, f32), around: Option<Rect>) {
        let Some(on_context) = &self.on_context else {
            return;
        };
        self.cursor.set(Some(row.node.id.clone()));
        on_context.run(TreeContext {
            node: row.node.clone(),
            at,
            around,
        });
    }

    fn row_rect(&self, id: &K) -> Option<Rect> {
        let node = focus::node_of(self.built.get(id)?)?;
        let laid = telar::track_layout(node)?;
        Some(telar::anchor_rect(node, &laid))
    }

    fn take_focus(&self) {
        if let Some(id) = self.focus.get() {
            focus::request_from_pointer(id);
        }
    }

    fn seek(&self, c: char, rows: &[Rc<Row<K>>], at: Option<usize>) {
        let needle = self.search.borrow_mut().extend(c);
        if let Some(i) = needle.find(at, rows.len(), |i| Some(rows[i].node.label.to_string())) {
            self.move_to(rows, i);
        }
    }

    fn on_key(&self, key: &Key) -> bool {
        let rows = self.rows();
        let at = self.at(&rows);
        if let Some(on_key) = &self.on_key
            && on_key(key, at.map(|i| &rows[i].node))
        {
            return true;
        }
        if rows.is_empty() {
            return false;
        }
        let last = rows.len() - 1;
        if is_context_menu_key(key) && self.on_context.is_some() {
            let i = at.unwrap_or(0);
            self.move_to(&rows, i);
            let around = self.row_rect(&rows[i].node.id).or_else(focused_rect);
            self.ask_context(&rows[i], (0.0, 0.0), around);
            return true;
        }
        let modifiers = telar::modifiers();
        let held = modifiers.is_shift || modifiers.is_ctrl || modifiers.is_alt || modifiers.is_meta;
        if held && walks(key) {
            return false;
        }
        let (opens, shuts) = match current_direction() {
            Direction::Rtl => (NamedKey::ArrowLeft, NamedKey::ArrowRight),
            _ => (NamedKey::ArrowRight, NamedKey::ArrowLeft),
        };
        match key {
            Key::Named(NamedKey::ArrowDown) => {
                self.move_to(&rows, at.map_or(0, |i| (i + 1).min(last)))
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.move_to(&rows, at.map_or(0, |i| i.saturating_sub(1)))
            }
            Key::Named(NamedKey::Home) => self.move_to(&rows, 0),
            Key::Named(NamedKey::End) => self.move_to(&rows, last),
            Key::Named(paging @ (NamedKey::PageDown | NamedKey::PageUp)) => {
                let forward = *paging == NamedKey::PageDown;
                let to = self.places.page(Some(at.unwrap_or(0)), rows.len(), forward);
                self.move_to(&rows, to.unwrap_or(0));
            }
            Key::Named(named) if *named == opens => {
                let i = at.unwrap_or(0);
                let row = &rows[i];
                if !row.node.is_branch() {
                    self.move_to(&rows, i);
                } else if self.is_open(&row.node.id) {
                    let into = rows.get(i + 1).filter(|next| next.parent == Some(i));
                    self.move_to(&rows, into.map_or(i, |_| i + 1));
                } else {
                    self.move_to(&rows, i);
                    self.open(&row.node.id);
                }
            }
            Key::Named(named) if *named == shuts => {
                let i = at.unwrap_or(0);
                let row = &rows[i];
                if row.node.is_branch() && self.is_open(&row.node.id) {
                    self.move_to(&rows, i);
                    self.shut(&row.node);
                } else {
                    self.move_to(&rows, row.parent.unwrap_or(i));
                }
            }
            // A space is a character only inside a query already under way, and a selection every other time.
            Key::Named(NamedKey::Space) if self.search.borrow().is_searching() => {
                self.seek(' ', &rows, at)
            }
            Key::Named(NamedKey::Enter | NamedKey::Space) => {
                if let Some(i) = at {
                    self.activate(&rows, i);
                }
            }
            // Ctrl or Meta makes a character a command rather than a query; Alt is how many layouts type one.
            Key::Char(c) if !c.is_control() && !modifiers.is_ctrl && !modifiers.is_meta => {
                self.seek(*c, &rows, at)
            }
            _ => return false,
        }
        true
    }
}

/// Whether `key` is one the tree walks its rows with, which it takes bare only.
fn walks(key: &Key) -> bool {
    matches!(
        key,
        Key::Named(
            NamedKey::ArrowDown
                | NamedKey::ArrowUp
                | NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::Home
                | NamedKey::End
                | NamedKey::PageUp
                | NamedKey::PageDown
        )
    )
}

fn build_row<K: RowKey>(
    tree: &Rc<Tree<K>>,
    row: Rc<Row<K>>,
    slots: &RowSlots<K>,
    color: &Option<Reactive<Color>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let id = row.node.id.clone();
    let mut content: Vec<Box<dyn LayoutItem>> = vec![caret(tree, &row)?];
    if let Some(leading) = &slots.leading {
        content.push(leading(&row.node)?);
    }
    let label = row.node.label.clone();
    let (size, wrap) = (tree.places.size(), tree.places.text_wrap());
    let text = Text::declaring(
        move || label.to_string(),
        LayoutStyle::new()
            .flex_grow(1.0)
            .flex_shrink(1.0)
            .min_width(0.0),
        move |t| shared::control_text(size, t, 1.0).with_text_wrap(wrap),
    )?;
    content.push(box_item(ClippedItem::new(box_item(text), Clip::both())));
    if let Some(badge) = row.node.badge.clone() {
        content.push(box_item(Text::declaring(
            move || badge.to_string(),
            LayoutStyle::new().flex_shrink(0.0),
            move |t| shared::quiet(size, t, shared::CAPTION_RATIO).with_text_wrap(TextWrap::NoWrap),
        )?));
    }
    if let Some(trailing) = &slots.trailing {
        content.push(trailing(&row.node)?);
    }

    let level = row.position.level;
    let paint = |hovered: bool| {
        let (tree, id, color) = (tree.clone(), id.clone(), color.clone());
        move |_r| row_paint(&tree, &id, &color, hovered)
    };
    let cursor = {
        let (tree, id, color) = (tree.clone(), id.clone(), color.clone());
        move || tree.shows_cursor(&id).then(|| cursor_ring(&color))
    };
    let selected = {
        let (tree, id) = (tree.clone(), id.clone());
        move || tree.is_selected(&id)
    };
    let press = {
        let (tree, id) = (tree.clone(), id.clone());
        move || {
            tree.activate_id(&id);
            tree.take_focus();
        }
    };
    let reach = {
        let (tree, id) = (tree.clone(), id.clone());
        move || tree.reach(&id)
    };
    let built = StyledContainer::new(row_box(level, &tree.places), paint(false), content)?
        .hover_style(paint(true))
        .keyboard_cursor(cursor)
        .presented(Role::TreeItem, reach)
        .toggled(selected)
        .positioned(row.position)
        .on_press(press)
        .maybe_on_alt_press(tree.on_context.is_some().then(|| {
            let (tree, row) = (tree.clone(), row.clone());
            Handler::from(move |button: PointerButton| {
                if button == PointerButton::Secondary {
                    tree.take_focus();
                    let at = telar::pointer_position().unwrap_or_default();
                    tree.ask_context(&row, at, None);
                }
            })
        }));
    let built = if row.node.is_branch() {
        let (tree, id) = (tree.clone(), id.clone());
        built.expanded(move || tree.is_open(&id))
    } else {
        built
    };
    if let Some(focus_id) = built.focus_id() {
        tree.built.track(id, focus_id);
    }
    let built = match &tree.drag {
        Some(drag) => Rc::clone(drag).item(built, &row.node),
        None => built,
    };
    Ok(box_item(built))
}

/// The open-or-shut chevron, or the empty column a leaf keeps so its label lines up with its siblings'. Pressing it toggles the branch without selecting the row, and a reader hears the row's state rather than the mark.
fn caret<K: RowKey>(tree: &Rc<Tree<K>>, row: &Row<K>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let size = tree.places.size();
    let column = caret_column(size, tree.places.line_height());
    if !row.node.is_branch() {
        return Ok(box_item(Container::new(column, vec![])?));
    }
    let point = {
        let (tree, id) = (tree.clone(), row.node.id.clone());
        move || match tree.is_open(&id) {
            true => shared::Mark::ChevronDown,
            false => shared::Mark::ChevronForward,
        }
    };
    let mark = shared::mark(size, point, |text| text.color.faded(shared::QUIET_ALPHA))?;
    let toggle = {
        let (tree, id) = (tree.clone(), row.node.id.clone());
        move || {
            tree.toggle_id(&id);
            tree.take_focus();
        }
    };
    let caret = Container::new(column, vec![mark])?
        .role(Role::Group)
        .on_press(toggle)
        .a11y_hidden();
    Ok(box_item(caret))
}

/// The caret's column, an icon wide and a line tall where it takes room, and at least the smallest pointer target where it is pressed: toward the label no further than the gap before it, which the label would otherwise cover, and the rest toward the row's start.
fn caret_column(size: ControlSize, line: f32) -> LayoutStyle {
    let side = shared::icon_size(size);
    let grown = shared::target_outset(side) * 2.0;
    let toward_label = (grown / 2.0).min(shared::gap_sm(size));
    let down = shared::target_outset(line);
    // Grown by negative margins rather than a hit outset: a plain container draws nothing, where the styled one an outset needs would add a box to the row's drawing.
    let toward_start = grown - toward_label;
    LayoutStyle::row()
        .width(side + grown)
        .height(line + down * 2.0)
        .margin_inline_start(-toward_start)
        .margin_inline_end(-toward_label)
        .padding_start(toward_start)
        .padding_end(toward_label)
        .margin_block_start(-down)
        .margin_block_end(-down)
        .flex_shrink(0.0)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
}

/// How far in from the tree's start a row of `level` starts what it shows.
fn indent_of(level: NonZeroU32, size: ControlSize) -> f32 {
    let tokens = TreeViewTokens::current();
    tokens.pad_x(size) + (level.get() - 1) as f32 * tokens.indent(size)
}

fn row_box(level: NonZeroU32, places: &RowPlaces) -> LayoutStyle {
    let size = places.size();
    places.sized(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .padding_start(indent_of(level, size))
            .padding_end(TreeViewTokens::current().pad_x(size))
            .gap(shared::gap_sm(size)),
    )
}

/// The selection and the pointer wash the row, at the theme's strengths and in the tree's colour when it was given one; a row a payload would be dropped onto is held and ringed instead.
fn row_paint<K: RowKey>(
    tree: &Tree<K>,
    id: &K,
    color: &Option<Reactive<Color>>,
    hovered: bool,
) -> RectStyle {
    let radius = TreeViewTokens::current().radius();
    if tree.takes_drop_onto(id) {
        return RectStyle::default()
            .with_fill(shared::held_wash())
            .with_border(Border::uniform(
                shared::accent(),
                shared::border_width_strong(),
            ))
            .with_radius(BorderRadius::all(radius));
    }
    row_wash(
        radius,
        color,
        RowState {
            selected: tree.is_selected(id),
            hovered,
        },
    )
}

/// The rule drawn at the gap a payload would land in, set in to the level of the nodes it lands among, over the rows laid out in `list`.
fn drop_line<K: RowKey>(
    tree: &Rc<Tree<K>>,
    list: telar::NodeId,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let laid = track_layout(list).ok_or_else(|| {
        LayoutError::Engine("tree view: its rows' layout node is not in the live tree".into())
    })?;
    let gap: Rc<dyn Fn() -> Option<usize>> = {
        let tree = Rc::clone(tree);
        Rc::new(
            move || match tree.drag.as_ref().and_then(|drag| drag.place())? {
                DropPlace::Between(index) => Some(index),
                DropPlace::Inside(_) => None,
            },
        )
    };
    let line_box = {
        let (tree, gap) = (Rc::clone(tree), Rc::clone(&gap));
        move || {
            let size = tree.places.size();
            let level = gap()
                .and_then(|index| tree.flat.with(|flat| flat.0.get(index).map(|row| row.position.level)))
                .unwrap_or(NonZeroU32::MIN);
            let inset = TreeViewTokens::current().inset(size);
            LayoutStyle::new()
                .absolute()
                .inset_top(0.0)
                .inset_start(inset + indent_of(level, size))
                .inset_end(inset)
                .height(shared::focus_ring_width())
        }
    };
    let line = StyledContainer::new(
        line_box(),
        {
            let gap = Rc::clone(&gap);
            move |_| match gap() {
                Some(_) => RectStyle::default().with_fill(shared::accent()),
                None => RectStyle::default(),
            }
        },
        vec![],
    )?
    .styled_by(line_box)
    .input_transparent()
    .with_transform({
        let tree = Rc::clone(tree);
        move |rect| {
            let index = gap()?;
            let count = tree.flat.with(|flat| flat.0.len());
            let top = match index.checked_sub(1) {
                Some(before) if index >= count => {
                    let (top, height) = tree.places.span(before);
                    top + height
                }
                _ => tree.places.span(index).0,
            };
            let along = laid.get().y + top - shared::focus_ring_width() / 2.0;
            Some(Transform::translate(0.0, along - rect.y))
        }
    });
    Ok(box_item(line))
}

#[cfg(test)]
#[path = "tree_view.test.rs"]
mod tests;
