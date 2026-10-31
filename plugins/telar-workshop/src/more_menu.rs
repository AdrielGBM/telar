//! A menu of tools behind a ⋯ button, or asked for at the pointer: what a header with no room for its tools folds them into.

use std::rc::Rc;

use telar::{
    Children, Container, LayoutError, LayoutItem, LayoutStyle, NodeId, Reactive, ReactiveList,
    Rect, RwSignal, absolute_rect, box_item, signal,
};
use telar_components::{ContextMenuProps, IconButtonProps, context_menu, icon_button};
use telar_icons::icon;

use crate::strings;

/// Where a menu was asked for: under a button, or at the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Asked {
    pub(crate) at: (f32, f32),
    pub(crate) around: Option<Rect>,
}

/// A ⋯ button named by the string `label` names, and the menu of `rows` it opens under itself.
pub(crate) fn more_button(
    label: &'static str,
    rows: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let open = signal(None::<Asked>);
    let anchor = signal(None::<NodeId>);
    let button = icon_button(
        IconButtonProps::props()
            .icon(icon!("lucide:ellipsis"))
            .label(Reactive::of(move || strings::text(label)))
            .on_press(Rc::new(move || {
                open.set(anchor.peek().and_then(absolute_rect).map(|around| Asked {
                    at: (around.x, around.y + around.height),
                    around: Some(around),
                }));
            }))
            .build(),
    )?;
    anchor.set(Some(button.layout_node()));
    Ok(box_item(Container::new(
        LayoutStyle::row().flex_shrink(0.0),
        vec![button, menu_layer(open, rows)?],
    )?))
}

/// The menu of `rows` while `open` says where it was asked for, closing it back to `None`.
pub(crate) fn menu_layer(
    open: RwSignal<Option<Asked>>,
    rows: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let menu = ReactiveList::new(
        move || vec![open.with(Option::is_some)],
        |shown: &bool| *shown,
        move |shown| {
            let Some(asked) = open.peek().filter(|_| shown) else {
                return Ok(box_item(Container::new(LayoutStyle::new(), Vec::new())?));
            };
            let props = ContextMenuProps::props()
                .on_close(Rc::new(move || open.set(None)))
                .at(asked.at);
            let props = match asked.around {
                Some(around) => props.around(around),
                None => props,
            };
            context_menu(props.children(rows.clone()).build())
        },
        0.0,
    )?;
    Ok(box_item(menu))
}
