//! A menu of tools behind a ⋯ button, or asked for at the pointer: what a header with no room for its tools folds them into.

use std::rc::Rc;

use telar::{
    Children, Container, LayoutError, LayoutItem, LayoutStyle, NodeId, Reactive, absolute_rect,
    box_item, signal,
};
use telar_components::{IconButtonProps, MenuPlace, icon_button, menu_layer};
use telar_icons::icon;

use crate::strings;

/// A ⋯ button named by the string `label` names, and the menu of `rows` it opens under itself.
pub(crate) fn more_button(
    label: &'static str,
    rows: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let open = signal(None::<MenuPlace>);
    let anchor = signal(None::<NodeId>);
    let button = icon_button(
        IconButtonProps::props()
            .icon(icon!("lucide:ellipsis"))
            .label(Reactive::of(move || strings::text(label)))
            .on_press(Rc::new(move || {
                open.set(anchor.peek().and_then(absolute_rect).map(MenuPlace::Under));
            }))
            .build(),
    )?;
    anchor.set(Some(button.layout_node()));
    Ok(box_item(Container::new(
        LayoutStyle::row().flex_shrink(0.0),
        vec![
            button,
            menu_layer(open, |place| *place, move |_| rows.clone())?,
        ],
    )?))
}
