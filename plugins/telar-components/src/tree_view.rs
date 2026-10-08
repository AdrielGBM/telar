//! [`tree_view`]: a hierarchy of rows, opened, closed and walked from the keyboard, with only the rows on screen built.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use telar::{
    Accessible, AlignItems, Border, BorderRadius, Children, Clip, ClippedItem, Color, Container,
    Direction, JustifyContent, Key, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, Memo,
    NamedKey, Props, Reactive, RectStyle, RwSignal, ScrollViewport, ShapeStyle, StyledContainer,
    Text, TextWrap, VirtualList, box_item, current_direction,
    focus::{self, FocusId, Role, SetPosition},
    memo, on_cleanup, signal,
};

use crate::shared;
use crate::type_ahead::TypeAhead;

const CARET_RATIO: f32 = 0.85;
const CARET_CLOSED: &str = "\u{25B8}";
const CARET_CLOSED_RTL: &str = "\u{25C2}";
const CARET_OPEN: &str = "\u{25BE}";
const SELECTED_ALPHA: f32 = 0.14;
const HOVER_ALPHA: f32 = 0.08;
const CURSOR_RING: f32 = 2.0;
const OVERSCAN: usize = 4;

fn row_height() -> f32 {
    (shared::spacing() * 3.5).round()
}
fn indent() -> f32 {
    shared::spacing() * 2.0
}
fn caret_width() -> f32 {
    shared::spacing() * 2.0
}
fn pad_x() -> f32 {
    shared::spacing()
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
}

/// A tree of rows given as data, walked with the arrow keys, Home, End and type-ahead, and bound to a selection and a set of open branches.
///
/// It fills the height its parent gives it and scrolls within it, building only the rows on screen, so a tree of thousands of nodes costs what a screenful does.
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
    /// Whether a branch can be selected. Off, a click, Enter or Space on a branch opens or shuts it instead, for a tree whose selection only ever names a leaf. On by default.
    #[props(default = true)]
    pub select_branches: bool,
    /// Builds what a row shows after its badge: a status dot, an action.
    #[props(some, default)]
    pub trailing: Option<TrailingBuilder>,
    /// What a reader calls the tree.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
    /// Accent for the selection and the keyboard cursor. `Color::TRANSPARENT` (the default) falls back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
}

/// What [`TreeViewProps::on_select`] is called with.
pub type SelectHandler = Rc<dyn Fn(&TreeNode)>;

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
        select_branches,
        trailing,
        label,
        color,
    } = props;
    let selected = selected.unwrap_or_else(|| signal(None));
    let expanded = expanded.unwrap_or_else(|| signal(HashSet::new()));
    let flat = memo(move || {
        let items = items.get();
        expanded.with(|open| Flat::of(&items, open))
    });
    let row_height = row_height();

    let mut made: Option<Rc<Tree>> = None;
    let scroll = LayoutScrollArea::new_with(fill(), |viewport| {
        let tree = Rc::new(Tree {
            flat,
            cursor: signal(None),
            selected,
            expanded,
            search: RefCell::default(),
            on_select,
            select_branches,
            viewport,
            row_height,
            focus: Cell::new(None),
            built: signal(HashMap::new()),
        });
        made = Some(tree.clone());
        let source = {
            let tree = tree.clone();
            move || tree.flat.with(|flat| flat.0.as_ref().clone())
        };
        let rows = VirtualList::new(
            LayoutStyle::new().flex_column(),
            tree.viewport.clone(),
            row_height,
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
        Ok(box_item(rows))
    })?;
    let tree = made.expect("a scroll area builds its content before it returns");

    let seeding = tree.clone();
    let keys = tree.clone();
    let cursor = tree.clone();
    let container = StyledContainer::new(
        fill().flex_column(),
        |_r| RectStyle::default(),
        vec![box_item(scroll)],
    )?
    .control(Role::Tree)
    .focus_style(|_r| RectStyle::default())
    .on_focus(move |now| {
        if now {
            seeding.seed_cursor();
        }
    })
    .active_descendant(move || cursor.cursor_row())
    .on_focused_key(move |key: &Key| keys.on_key(key));
    tree.focus.set(container.focus_id());
    Ok(match label {
        Some(label) => box_item(container.a11y_label(move || label.get())),
        None => box_item(container),
    })
}

fn fill() -> LayoutStyle {
    LayoutStyle::new().flex_grow(1.0).min_height(0.0)
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
    select_branches: bool,
    viewport: ScrollViewport,
    row_height: f32,
    focus: Cell<Option<FocusId>>,
    /// The presented control of each row that is built, by node id: what the cursor names to a reader.
    built: RwSignal<HashMap<Arc<str>, FocusId>>,
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
        let id = self.cursor.get()?;
        self.built.with(|built| built.get(&id).copied())
    }

    fn row_built(&self, id: Arc<str>, row: FocusId) {
        self.built.update(|built| {
            built.insert(id, row);
        });
    }

    fn row_dropped(&self, id: &str, row: FocusId) {
        if self.built.is_alive() && self.built.peek_with(|built| built.get(id) == Some(&row)) {
            self.built.update(|built| {
                built.remove(id);
            });
        }
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

    /// Scrolls the least distance that brings row `index` fully into view, worked out from the fixed row height because the row may not be built.
    fn reveal(&self, index: usize) {
        let (x, offset) = self.viewport.peek_offset();
        let height = self.viewport.rect().peek().height;
        if height <= 0.0 {
            return;
        }
        let top = index as f32 * self.row_height;
        let bottom = top + self.row_height;
        let target = if top < offset {
            top
        } else if bottom > offset + height {
            bottom - height
        } else {
            return;
        };
        self.viewport.scroll_to(x, target);
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
    let text = Text::declaring(
        move || label.to_string(),
        LayoutStyle::new()
            .flex_grow(1.0)
            .flex_shrink(1.0)
            .min_width(0.0),
        |t| shared::control_text(t, 1.0).with_text_wrap(TextWrap::NoWrap),
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
    let built = StyledContainer::new(row_box(level, tree.row_height), paint(false), content)?
        .hover_style(paint(true))
        .presented(Role::TreeItem)
        .toggled(selected)
        .positioned(row.position)
        .on_press(press);
    let built = if row.node.is_branch() {
        let (tree, id) = (tree.clone(), id.clone());
        built.expanded(move || tree.is_open(&id))
    } else {
        built
    };
    if let Some(focus_id) = built.focus_id() {
        tree.row_built(id.clone(), focus_id);
        let tree = tree.clone();
        on_cleanup(move || tree.row_dropped(&id, focus_id));
    }
    Ok(box_item(built))
}

/// The open-or-shut glyph, or the empty column a leaf keeps so its label lines up with its siblings'. Pressing it toggles the branch without selecting the row, and a reader hears the row's state rather than the glyph.
fn caret(tree: &Rc<Tree>, row: &Row) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let column = LayoutStyle::new()
        .width(caret_width())
        .height(tree.row_height)
        .flex_shrink(0.0)
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER);
    if !row.node.is_branch() {
        return Ok(box_item(Container::new(column, vec![])?));
    }
    let glyph = {
        let (tree, id) = (tree.clone(), row.node.id.clone());
        move || {
            match (tree.is_open(&id), current_direction()) {
                (true, _) => CARET_OPEN,
                (false, Direction::Rtl) => CARET_CLOSED_RTL,
                (false, _) => CARET_CLOSED,
            }
            .to_string()
        }
    };
    let text = Text::declaring(glyph, LayoutStyle::new(), |t| shared::quiet(t, CARET_RATIO))?;
    let toggle = {
        let (tree, id) = (tree.clone(), row.node.id.clone());
        move || {
            tree.toggle_id(&id);
            tree.take_focus();
        }
    };
    let caret = StyledContainer::new(column, |_r| RectStyle::default(), vec![box_item(text)])?
        .role(Role::Group)
        .on_press(toggle)
        .a11y_hidden();
    Ok(box_item(caret))
}

fn row_box(level: u32, height: f32) -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .height(height)
        .flex_shrink(0.0)
        .padding_start(pad_x() + level.saturating_sub(1) as f32 * indent())
        .padding_end(pad_x())
        .gap(shared::spacing() * 0.5)
}

/// The selection fills the row; the keyboard cursor rings it while the tree shows focus, so the two read apart when they are on different rows.
fn row_paint(tree: &Tree, id: &str, color: &Reactive<Color>, hovered: bool) -> RectStyle {
    let accent = shared::resolve(color, shared::accent);
    let mut style = RectStyle::default().with_radius(BorderRadius::all(shared::radius_sm()));
    if tree.is_selected(id) {
        style = style.with_fill(accent.with_alpha(SELECTED_ALPHA));
    } else if hovered {
        style = style.with_fill(accent.with_alpha(HOVER_ALPHA));
    }
    if tree.shows_cursor(id) {
        style = style.with_border(Border::uniform(accent, CURSOR_RING));
    }
    style
}

#[cfg(test)]
#[path = "tree_view_test.rs"]
mod tests;
