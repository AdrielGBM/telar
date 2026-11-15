//! [`tab_panel`]: the panel a row of tabs shows the chosen tab's content in, joined to that row by a [`TabPanelLink`].

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use telar::focus::{self, FocusId, Role};
use telar::{
    Accessible, Children, LayoutError, LayoutItem, LayoutStyle, Props, RectStyle, StyledContainer,
    box_item,
};

/// What joins a row of tabs to the panel showing the chosen tab's content: a reader is told the chosen tab controls the panel, and the panel is called by that tab's name.
///
/// Made by the caller and handed to both: to the row as [`TabsProps::panel`](crate::TabsProps::panel) or [`tab_list`](crate::tab_list)'s `panel`, and to the [`tab_panel`] around the content. A clone is the same link.
#[derive(Clone, Default)]
pub struct TabPanelLink(Rc<Joined>);

#[derive(Default)]
struct Joined {
    panel: Cell<Option<FocusId>>,
    name: RefCell<Option<Rc<dyn Fn() -> String>>>,
}

impl TabPanelLink {
    /// A link that joins nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The panel the chosen tab controls, once it is built.
    pub(crate) fn panel(&self) -> Option<FocusId> {
        self.0.panel.get()
    }

    /// Calls the panel by what `name` reads: the chosen tab's name.
    pub(crate) fn name_by(&self, name: impl Fn() -> String + 'static) {
        *self.0.name.borrow_mut() = Some(Rc::new(name));
    }

    fn name(&self) -> String {
        let name = self.0.name.borrow().clone();
        name.map(|name| name()).unwrap_or_default()
    }
}

/// The panel showing the chosen tab's content.
#[derive(Props)]
pub struct TabPanelProps {
    /// The link the row of tabs was given.
    #[props(default)]
    pub link: TabPanelLink,
    /// The content shown, which is the caller's to switch as the chosen tab changes.
    #[props(default)]
    pub children: Children,
}

fn fill() -> LayoutStyle {
    LayoutStyle::column()
        .flex_grow(1.0)
        .min_width(0.0)
        .min_height(0.0)
}

/// A column that fills the room it is given and holds the content its tab shows: a tab panel, called by the name of the tab chosen in the row its [`TabPanelLink`] joins it to. It is no Tab stop; a reader asked to move to it takes the keyboard to the first control inside it.
pub fn tab_panel(props: TabPanelProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let TabPanelProps { link, children } = props;
    let panel =
        StyledContainer::new(fill(), |_r| RectStyle::default(), children.build()?)?.styled_by(fill);
    let node = panel.layout_node();
    let named = link.clone();
    let panel = panel
        .presented(Role::TabPanel, move || {
            focus::focus_first_in(node);
        })
        .a11y_label(move || named.name());
    link.0.panel.set(panel.focus_id());
    Ok(box_item(panel))
}

#[cfg(test)]
#[path = "tab_panel.test.rs"]
mod tests;
