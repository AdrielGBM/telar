//! [`toolbar`]: a row of controls that is one stop in the tab order, with the arrow keys moving between its items.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use telar::{
    Accessible, AlignItems, Children, ConsumedKeys, Direction, Key, LayoutError, LayoutItem,
    LayoutStyle, NamedKey, NodeId, Orientation, Props, Reactive, RectStyle, RwSignal,
    StyledContainer, box_item, current_direction,
    focus::{self, FocusId, Role},
    signal,
};

use crate::shared;

/// What an item needs from the toolbar it is in, and what the toolbar learns from its items.
///
/// The toolbar is the focus stop and the items are what its cursor rests on: the cursor is drawn by the item and moved by the toolbar's keys, so the tab order holds one entry however many items there are.
#[derive(Clone)]
pub(crate) struct ToolbarContext(Rc<State>);

struct State {
    items: RefCell<Vec<Item>>,
    cursor: RwSignal<usize>,
    focused: RwSignal<bool>,
    focus_id: Cell<Option<FocusId>>,
    node: Cell<Option<NodeId>>,
}

struct Item {
    enabled: Rc<dyn Fn() -> bool>,
    activate: Rc<dyn Fn()>,
    focus_id: Option<FocusId>,
}

impl ToolbarContext {
    fn new() -> Self {
        Self(Rc::new(State {
            items: RefCell::new(Vec::new()),
            cursor: signal(0),
            focused: signal(false),
            focus_id: Cell::new(None),
            node: Cell::new(None),
        }))
    }

    pub(crate) fn claim(
        &self,
        enabled: impl Fn() -> bool + 'static,
        activate: Rc<dyn Fn()>,
    ) -> usize {
        let mut items = self.0.items.borrow_mut();
        items.push(Item {
            enabled: Rc::new(enabled),
            activate,
            focus_id: None,
        });
        items.len() - 1
    }

    /// Names the presented control that stands for item `index`, which a reader is pointed at while the cursor rests on it.
    pub(crate) fn present(&self, index: usize, focus_id: Option<FocusId>) {
        if let Some(item) = self.0.items.borrow_mut().get_mut(index) {
            item.focus_id = focus_id;
        }
    }

    pub(crate) fn shows_cursor_at(&self, index: usize) -> bool {
        self.0.focused.get()
            && self.resolved_cursor() == Some(index)
            && self.0.focus_id.get().is_some_and(focus::is_focus_visible)
    }

    pub(crate) fn select(&self, index: usize) {
        self.0.cursor.set(index);
        if self.0.focused.peek() {
            return;
        }
        match self.0.focus_id.get() {
            Some(id) => focus::request_from_pointer(id),
            None => {
                if let Some(node) = self.0.node.get() {
                    focus::focus_first_in(node);
                }
            }
        }
    }

    fn enabled_at(&self, index: usize) -> bool {
        let enabled = self
            .0
            .items
            .borrow()
            .get(index)
            .map(|item| item.enabled.clone());
        enabled.is_some_and(|enabled| enabled())
    }

    fn cursor_item(&self) -> Option<FocusId> {
        let at = self.resolved_cursor()?;
        self.0.items.borrow().get(at).and_then(|item| item.focus_id)
    }

    fn len(&self) -> usize {
        self.0.items.borrow().len()
    }

    fn resolved_cursor(&self) -> Option<usize> {
        let at = self.0.cursor.get();
        if self.enabled_at(at) {
            return Some(at);
        }
        (0..self.len()).find(|&index| self.enabled_at(index))
    }

    fn move_to(&self, index: Option<usize>) -> bool {
        let Some(index) = index else { return false };
        self.0.cursor.set(index);
        true
    }

    fn step(&self, direction: isize) -> bool {
        let len = self.len() as isize;
        let Some(from) = self.resolved_cursor() else {
            return false;
        };
        let target = (1..len)
            .map(|offset| (from as isize + direction * offset).rem_euclid(len) as usize)
            .find(|&index| self.enabled_at(index));
        self.move_to(target)
    }

    fn activate(&self) -> bool {
        let Some(at) = self.resolved_cursor() else {
            return false;
        };
        let activate = self
            .0
            .items
            .borrow()
            .get(at)
            .map(|item| item.activate.clone());
        activate.is_some_and(|activate| {
            activate();
            true
        })
    }

    fn on_key(&self, key: &Key, vertical: bool) -> bool {
        let (back, forward) = match (vertical, current_direction()) {
            (true, _) => (NamedKey::ArrowUp, NamedKey::ArrowDown),
            (false, Direction::Rtl) => (NamedKey::ArrowRight, NamedKey::ArrowLeft),
            (false, _) => (NamedKey::ArrowLeft, NamedKey::ArrowRight),
        };
        let Key::Named(named) = key else { return false };
        match named {
            named if *named == forward => self.step(1),
            named if *named == back => self.step(-1),
            NamedKey::Home => self.move_to((0..self.len()).find(|&i| self.enabled_at(i))),
            NamedKey::End => self.move_to((0..self.len()).rev().find(|&i| self.enabled_at(i))),
            NamedKey::Enter | NamedKey::Space => self.activate(),
            _ => false,
        }
    }
}

/// A group of icon buttons laid out along one axis.
#[derive(Props)]
pub struct ToolbarProps {
    /// What the toolbar is for, for a screen reader: "Formatting", "Canvas".
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Lay the items out in a column, with the up and down arrows moving between them.
    #[props(default = false)]
    pub vertical: bool,
}

/// One tab stop for all its items: Tab enters and leaves, the arrows walk the items, Home and End jump to the first and last, Enter and Space press the current one.
///
/// Only [`icon_button`](crate::icon_button) takes part. A disabled item is skipped.
pub fn toolbar(
    props: ToolbarProps,
    children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ToolbarProps { label, vertical } = props;
    let context = ToolbarContext::new();
    let items = children.build_with(context.clone())?.take_default();

    let layout = move || {
        let style = LayoutStyle::new()
            .gap(shared::spacing() * 0.25)
            .align_items(AlignItems::CENTER);
        if vertical {
            style.flex_column()
        } else {
            style.flex_row()
        }
    };
    let keys = if vertical {
        ConsumedKeys::VERTICAL_ARROWS
    } else {
        ConsumedKeys::HORIZONTAL_ARROWS
    }
    .with(ConsumedKeys::EDGES)
    .with(ConsumedKeys::ACTIVATION);

    let on_key_context = context.clone();
    let on_focus_context = context.clone();
    let cursor_context = context.clone();
    let bar = StyledContainer::new(layout(), |_| RectStyle::default(), items)?
        .styled_by(layout)
        .control(Role::Toolbar)
        .oriented(if vertical {
            Orientation::Vertical
        } else {
            Orientation::Horizontal
        })
        .focus_style(|_| RectStyle::default())
        .consumes_keys(move || keys)
        .active_descendant(move || cursor_context.cursor_item())
        .on_focused_key(move |key| on_key_context.on_key(key, vertical))
        .on_focus(move |focused| {
            if focused {
                on_focus_context.0.focus_id.set(focus::current());
            }
            on_focus_context.0.focused.set(focused);
        })
        .a11y_label(move || label.get());
    context.0.node.set(Some(bar.layout_node()));
    Ok(box_item(bar))
}

#[cfg(test)]
#[path = "toolbar_test.rs"]
mod tests;
