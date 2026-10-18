//! [`tree_view`]: a hierarchy of rows, opened, closed and walked from the keyboard, with only the rows on screen built.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use telar::{
    Accessible, AlignItems, Children, Clip, ClippedItem, Color, Container, Direction,
    JustifyContent, Key, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, Memo, NamedKey,
    PointerButton, Props, Reactive, Rect, RectStyle, RwSignal, StyledContainer, Text, TextWrap,
    box_item, current_direction,
    focus::{self, FocusId, Role, SetPosition},
    memo, signal,
};

use crate::menu::{focused_rect, is_context_menu_key};
use crate::rows::{BuiltRows, RowPlaces, RowState, cursor_row_paint};
use crate::shared;
use crate::type_ahead::TypeAhead;

const OVERSCAN: usize = 4;

/// A tree view's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the tree is built or painted, so a tree's rows stand at the height every other list's do.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TreeViewTokens {
    /// Height of a row at the regular control size, scaled with the control size in force. `None` is [`metrics::row_height`](crate::metrics::row_height).
    pub row_height: Option<f32>,
    /// How far each level is set in from the one above it, likewise. `None` is [`metrics::gap_xl`](crate::metrics::gap_xl).
    pub indent: Option<f32>,
    /// Corner of a row's wash and its cursor. `None` is the theme's `radius_item`.
    pub radius: Option<f32>,
    /// Space between a row's edges and what it shows, likewise. `None` is [`metrics::gap_md`](crate::metrics::gap_md).
    pub pad_x: Option<f32>,
    /// Space between the tree's sides and its rows, so a row's wash stands clear of them as a menu's does of its panel, likewise. `None` is [`metrics::panel_pad`](crate::metrics::panel_pad).
    pub inset: Option<f32>,
}

impl TreeViewTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    /// In whole pixels: a virtual list places the rows it has not built by multiplying it, and a fractional height drifts off the pixels layout snaps the built ones to.
    pub fn row_height(&self) -> f32 {
        self.row_height
            .map_or_else(shared::row_height, |height| height * telar::control_scale())
            .round()
    }

    pub fn indent(&self) -> f32 {
        self.indent
            .map_or_else(shared::gap_xl, |indent| indent * telar::control_scale())
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_item)
    }

    pub fn pad_x(&self) -> f32 {
        self.pad_x
            .map_or_else(shared::gap_md, |pad| pad * telar::control_scale())
    }

    pub fn inset(&self) -> f32 {
        self.inset
            .map_or_else(shared::panel_pad, |inset| inset * telar::control_scale())
    }
}

/// One node of a tree, as data: what it is called, the id the selection and the open branches name it by, and the nodes under it.
///
/// The children sit behind an `Arc`, so handing a whole tree to a signal or cloning one row out of it copies pointers rather than the hierarchy.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TreeNode {
    /// Unique across the whole tree.
    pub id: Arc<str>,
    pub label: Arc<str>,
    /// Quiet trailing text: a count, a status.
    pub badge: Option<Arc<str>>,
    pub children: Arc<[TreeNode]>,
}

impl TreeNode {
    pub fn new(id: impl Into<Arc<str>>, label: impl Into<Arc<str>>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            badge: None,
            children: Arc::new([]),
        }
    }

    pub fn with_badge(mut self, badge: impl Into<Arc<str>>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    pub fn with_children(mut self, children: impl IntoIterator<Item = TreeNode>) -> Self {
        self.children = children.into_iter().collect();
        self
    }

    /// Whether the node has nodes under it, which is what makes it something to open.
    pub fn is_branch(&self) -> bool {
        !self.children.is_empty()
    }

    /// The id of every branch among `nodes` and under them, at any depth, parents before what they hold: what opening them all opens.
    pub fn branch_ids(nodes: &[TreeNode]) -> Vec<Arc<str>> {
        let mut ids = Vec::new();
        collect_branch_ids(nodes, &mut ids);
        ids
    }
}

fn collect_branch_ids(nodes: &[TreeNode], ids: &mut Vec<Arc<str>>) {
    for node in nodes.iter().filter(|node| node.is_branch()) {
        ids.push(node.id.clone());
        collect_branch_ids(&node.children, ids);
    }
}

/// A tree of rows given as data, walked with the arrow keys, Home, End, Page Up, Page Down and type-ahead, and bound to a selection and a set of open branches.
///
/// It fills the height its parent gives it and scrolls within it, building only the rows on screen, so a tree of thousands of nodes costs what a screenful does. Every row stands at [`TreeViewTokens::row_height`], unless [`wrap`](Self::wrap) lets rows grow: then each is measured once built, and stands at the row height until then.
#[derive(Props)]
pub struct TreeViewProps {
    #[props(into, default)]
    pub items: Reactive<Vec<TreeNode>>,
    /// The bound selection, by node id. `None` (the default) is uncontrolled.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<Option<Arc<str>>>>,
    /// The bound set of open branches, by node id. `None` (the default) is uncontrolled and starts with every branch shut.
    #[props(some, into, default)]
    pub expanded: Option<RwSignal<HashSet<Arc<str>>>>,
    /// Fired with the node a click, Enter or Space chose.
    #[props(some, default)]
    pub on_select: Option<SelectHandler>,
    /// Fired when a row's context menu is asked for — a secondary press on the row, or the context-menu key or Shift+F10 on the row the keyboard is on — with the row and where the menu opens. The row takes the keyboard cursor first, so it reads as the one the menu is about. `None` (the default) leaves a secondary press to what is behind the tree.
    #[props(some, default)]
    pub on_context: Option<ContextHandler>,
    /// Whether a branch can be selected. Off, a click, Enter or Space on a branch opens or shuts it instead, for a tree whose selection only ever names a leaf. On by default.
    #[props(default = true)]
    pub select_branches: bool,
    /// Builds what a row shows after its badge: a status dot, an action.
    #[props(some, default)]
    pub trailing: Option<TrailingBuilder>,
    /// Whether a long label runs onto more lines instead of being cut off where its row ends, the row growing to hold it. Off (the default), every row is one row height tall, and nothing is measured.
    #[props(default)]
    pub wrap: bool,
    /// What a reader calls the tree.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
    /// Accent for the selection and the keyboard cursor. `Color::TRANSPARENT` (the default) falls back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
}

/// What [`TreeViewProps::on_select`] is called with.
pub type SelectHandler = Rc<dyn Fn(&TreeNode)>;

/// What a row's context menu was asked for on, and where it opens, as [`ContextMenuProps`](crate::ContextMenuProps) takes it: at the pointer for a secondary press, or under the row for one asked for from the keyboard.
#[derive(Clone, Debug)]
pub struct TreeContext {
    pub node: TreeNode,
    pub at: (f32, f32),
    pub around: Option<Rect>,
}

pub type ContextHandler = Rc<dyn Fn(TreeContext)>;

/// What [`TreeViewProps::trailing`] builds a row's trailing content with.
pub type TrailingBuilder = Rc<dyn Fn(&TreeNode) -> Result<Box<dyn LayoutItem>, LayoutError>>;

/// A tree of rows given as data, with expand and collapse, keyboard navigation, type-ahead and a bound selection.
pub fn tree_view(
    props: TreeViewProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let TreeViewProps {
        items,
        selected,
        expanded,
        on_select,
        on_context,
        select_branches,
        trailing,
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
    let row_height = TreeViewTokens::current().row_height();

    let mut made: Option<Rc<Tree>> = None;
    let scroll = LayoutScrollArea::new_with(fill(), |viewport| {
        let tree = Rc::new(Tree {
            flat,
            cursor: signal(None),
            selected,
            expanded,
            search: RefCell::default(),
            on_select,
            on_context,
            select_branches,
            places: RowPlaces::new(viewport, row_height, wrap),
            focus: Cell::new(None),
            built: BuiltRows::new(),
        });
        made = Some(tree.clone());
        let source = {
            let tree = tree.clone();
            move || tree.flat.with(|flat| flat.0.as_ref().clone())
        };
        let rows = tree.places.clone().list(
            OVERSCAN,
            source,
            |row: &Rc<Row>| {
                (
                    row.node.id.clone(),
                    row.node.label.clone(),
                    row.node.badge.clone(),
                    row.node.is_branch(),
                    row.position,
                )
            },
            move |_, row| build_row(&tree, row, trailing.as_ref(), &color),
        )?;
        let inset = Container::new(inset_box(), vec![box_item(rows)])?.styled_by(inset_box);
        Ok(box_item(inset))
    })?;
    let tree = made.expect("a scroll area builds its content before it returns");

    let seeding = tree.clone();
    let keys = tree.clone();
    let cursor = tree.clone();
    let scroll = scroll
        .control(Role::Tree)
        .focus_style(|_r| RectStyle::default())
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
fn inset_box() -> LayoutStyle {
    LayoutStyle::column().padding_horizontal(TreeViewTokens::current().inset())
}

struct Row {
    node: TreeNode,
    position: SetPosition,
    parent: Option<usize>,
}

/// The rows the open branches show, in order. Equal only to itself, so the memo notifies on every rebuild instead of comparing whole trees.
#[derive(Clone)]
struct Flat(Rc<Vec<Rc<Row>>>);

impl PartialEq for Flat {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Flat {
    fn of(items: &[TreeNode], open: &HashSet<Arc<str>>) -> Self {
        let mut rows = Vec::new();
        push_level(&mut rows, items, open, 1, None);
        Self(Rc::new(rows))
    }
}

fn push_level(
    rows: &mut Vec<Rc<Row>>,
    nodes: &[TreeNode],
    open: &HashSet<Arc<str>>,
    level: u32,
    parent: Option<usize>,
) {
    let size = nodes.len() as u32;
    for (i, node) in nodes.iter().enumerate() {
        let at = rows.len();
        rows.push(Rc::new(Row {
            node: node.clone(),
            position: SetPosition {
                level,
                position: i as u32 + 1,
                size,
            },
            parent,
        }));
        if node.is_branch() && open.contains(&node.id) {
            push_level(rows, &node.children, open, level + 1, Some(at));
        }
    }
}

struct Tree {
    flat: Memo<Flat>,
    /// The row the keyboard is on. Deliberately not the selection: walking the tree commits nothing.
    cursor: RwSignal<Option<Arc<str>>>,
    selected: RwSignal<Option<Arc<str>>>,
    expanded: RwSignal<HashSet<Arc<str>>>,
    search: RefCell<TypeAhead>,
    on_select: Option<SelectHandler>,
    on_context: Option<ContextHandler>,
    select_branches: bool,
    places: RowPlaces,
    focus: Cell<Option<FocusId>>,
    built: BuiltRows<Arc<str>>,
}

impl Tree {
    fn rows(&self) -> Rc<Vec<Rc<Row>>> {
        self.flat.with(|flat| flat.0.clone())
    }

    fn index_of(rows: &[Rc<Row>], id: Option<Arc<str>>) -> Option<usize> {
        let id = id?;
        rows.iter().position(|row| row.node.id == id)
    }

    /// Where the keys act from: the cursor while its row is showing, else the selection's row.
    fn at(&self, rows: &[Rc<Row>]) -> Option<usize> {
        Self::index_of(rows, self.cursor.peek())
            .or_else(|| Self::index_of(rows, self.selected.peek()))
    }

    fn is_open(&self, id: &str) -> bool {
        self.expanded.with(|open| open.contains(id))
    }

    fn is_selected(&self, id: &str) -> bool {
        self.selected
            .with(|selected| selected.as_deref() == Some(id))
    }

    fn shows_cursor(&self, id: &str) -> bool {
        self.focus.get().is_some_and(focus::is_focus_visible)
            && self.cursor.with(|cursor| cursor.as_deref() == Some(id))
    }

    fn cursor_row(&self) -> Option<FocusId> {
        self.built.get(&self.cursor.get()?)
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

    fn move_to(&self, rows: &[Rc<Row>], index: usize) {
        self.cursor.set(Some(rows[index].node.id.clone()));
        self.reveal(index);
    }

    fn reveal(&self, index: usize) {
        self.places.reveal(index);
    }

    /// What a click, Enter or Space does to the row at `index`: selects it, or opens or shuts it when it is a branch the tree does not select.
    fn activate(&self, rows: &[Rc<Row>], index: usize) {
        let row = &rows[index];
        if row.node.is_branch() && !self.select_branches {
            self.cursor.set(Some(row.node.id.clone()));
            self.toggle(rows, index);
        } else {
            self.select(row);
        }
    }

    fn activate_id(&self, id: &str) {
        let rows = self.rows();
        if let Some(index) = rows.iter().position(|row| &*row.node.id == id) {
            self.activate(&rows, index);
        }
    }

    fn select(&self, row: &Row) {
        let id = row.node.id.clone();
        self.cursor.set(Some(id.clone()));
        self.selected.set(Some(id));
        if let Some(on_select) = &self.on_select {
            on_select(&row.node);
        }
    }

    fn set_open(&self, id: &Arc<str>, open: bool) {
        self.expanded.update(|set| {
            if open {
                set.insert(id.clone());
            } else {
                set.remove(id);
            }
        });
    }

    /// Opens or shuts the branch at `index`; shutting one with the cursor inside moves the cursor onto it, where it would otherwise vanish.
    fn toggle(&self, rows: &[Rc<Row>], index: usize) {
        let row = &rows[index];
        if !self.is_open(&row.node.id) {
            self.set_open(&row.node.id, true);
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
        self.set_open(&row.node.id, false);
    }

    fn toggle_id(&self, id: &str) {
        let rows = self.rows();
        if let Some(index) = rows.iter().position(|row| &*row.node.id == id) {
            self.toggle(&rows, index);
        }
    }

    fn ask_context(&self, row: &Row, at: (f32, f32), around: Option<Rect>) {
        let Some(on_context) = &self.on_context else {
            return;
        };
        self.cursor.set(Some(row.node.id.clone()));
        on_context(TreeContext {
            node: row.node.clone(),
            at,
            around,
        });
    }

    fn row_rect(&self, id: &Arc<str>) -> Option<Rect> {
        let node = focus::node_of(self.built.get(id)?)?;
        let laid = telar::track_layout(node)?;
        Some(telar::anchor_rect(node, &laid))
    }

    fn take_focus(&self) {
        if let Some(id) = self.focus.get() {
            focus::request_from_pointer(id);
        }
    }

    fn seek(&self, c: char, rows: &[Rc<Row>], at: Option<usize>) {
        let needle = self.search.borrow_mut().extend(c);
        if let Some(i) = needle.find(at, rows.len(), |i| Some(rows[i].node.label.to_string())) {
            self.move_to(rows, i);
        }
    }

    fn on_key(&self, key: &Key) -> bool {
        let rows = self.rows();
        if rows.is_empty() {
            return false;
        }
        let last = rows.len() - 1;
        let at = self.at(&rows);
        if is_context_menu_key(key) && self.on_context.is_some() {
            let i = at.unwrap_or(0);
            self.move_to(&rows, i);
            let around = self.row_rect(&rows[i].node.id).or_else(focused_rect);
            self.ask_context(&rows[i], (0.0, 0.0), around);
            return true;
        }
        let (opens, shuts) = match current_direction() {
            Direction::Rtl => (NamedKey::ArrowLeft, NamedKey::ArrowRight),
            _ => (NamedKey::ArrowRight, NamedKey::ArrowLeft),
        };
        let modifiers = telar::modifiers();
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
                    self.move_to(&rows, i + 1);
                } else {
                    self.move_to(&rows, i);
                    self.set_open(&row.node.id, true);
                }
            }
            Key::Named(named) if *named == shuts => {
                let i = at.unwrap_or(0);
                let row = &rows[i];
                if row.node.is_branch() && self.is_open(&row.node.id) {
                    self.move_to(&rows, i);
                    self.set_open(&row.node.id, false);
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

fn build_row(
    tree: &Rc<Tree>,
    row: Rc<Row>,
    trailing: Option<&TrailingBuilder>,
    color: &Reactive<Color>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let id = row.node.id.clone();
    let mut content: Vec<Box<dyn LayoutItem>> = vec![caret(tree, &row)?];
    let label = row.node.label.clone();
    let wrap = tree.places.text_wrap();
    let text = Text::declaring(
        move || label.to_string(),
        LayoutStyle::new()
            .flex_grow(1.0)
            .flex_shrink(1.0)
            .min_width(0.0),
        move |t| shared::control_text(t, 1.0).with_text_wrap(wrap),
    )?;
    content.push(box_item(ClippedItem::new(box_item(text), Clip::both())));
    if let Some(badge) = row.node.badge.clone() {
        content.push(box_item(Text::declaring(
            move || badge.to_string(),
            LayoutStyle::new().flex_shrink(0.0),
            |t| shared::quiet(t, shared::CAPTION_RATIO).with_text_wrap(TextWrap::NoWrap),
        )?));
    }
    if let Some(trailing) = trailing {
        content.push(trailing(&row.node)?);
    }

    let level = row.position.level;
    let paint = |hovered: bool| {
        let (tree, id, color) = (tree.clone(), id.clone(), color.clone());
        move |_r| row_paint(&tree, &id, &color, hovered)
    };
    let selected = {
        let (tree, id) = (tree.clone(), id.clone());
        move || tree.is_selected(&id)
    };
    let press = {
        let (tree, row) = (tree.clone(), row.clone());
        move || {
            tree.activate_id(&row.node.id);
            tree.take_focus();
        }
    };
    let built = StyledContainer::new(row_box(level, &tree.places), paint(false), content)?
        .hover_style(paint(true))
        .presented(Role::TreeItem)
        .toggled(selected)
        .positioned(row.position)
        .on_press(press)
        .maybe_on_alt_press(tree.on_context.is_some().then(|| {
            let (tree, row) = (tree.clone(), row.clone());
            move |button: PointerButton| {
                if button == PointerButton::Secondary {
                    tree.take_focus();
                    let at = telar::pointer_position().unwrap_or_default();
                    tree.ask_context(&row, at, None);
                }
            }
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
    Ok(box_item(built))
}

/// The open-or-shut chevron, or the empty column a leaf keeps so its label lines up with its siblings'. Pressing it toggles the branch without selecting the row, and a reader hears the row's state rather than the mark.
fn caret(tree: &Rc<Tree>, row: &Row) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let column = LayoutStyle::row()
        .width(shared::icon_size())
        .height(tree.places.line_height())
        .flex_shrink(0.0)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER);
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
    let mark = shared::mark(point, |text| text.color.faded(shared::QUIET_ALPHA))?;
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

fn row_box(level: u32, places: &RowPlaces) -> LayoutStyle {
    let tokens = TreeViewTokens::current();
    places.sized(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .padding_start(tokens.pad_x() + level.saturating_sub(1) as f32 * tokens.indent())
            .padding_end(tokens.pad_x())
            .gap(shared::gap_sm()),
    )
}

/// The selection and the pointer wash the row, at the theme's strengths and in the tree's colour when it was given one; the keyboard cursor rings it while the tree shows focus, so the two read apart when they are on different rows.
fn row_paint(tree: &Tree, id: &str, color: &Reactive<Color>, hovered: bool) -> RectStyle {
    cursor_row_paint(
        TreeViewTokens::current().radius(),
        color,
        RowState {
            selected: tree.is_selected(id),
            hovered,
            cursor: tree.shows_cursor(id),
        },
    )
}

#[cfg(test)]
#[path = "tree_view_test.rs"]
mod tests;
