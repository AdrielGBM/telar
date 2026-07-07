use layout_core::LayoutError;
use renderer_core::Color;
use ui_core::LayoutItem;

use crate::dropdown;
// Re-exported for the test module below, which reads these via `use super::*` to compute click points.
#[cfg(test)]
use crate::dropdown::{PANEL_PAD, PANEL_WIDTH, ROW_HEIGHT, TRIGGER_HEIGHT};
#[cfg(test)]
use ui_core::track_layout;

/// A click-triggered list of action items: a labelled trigger button that opens an anchored list; picking an
/// item fires `on_select` with its index and closes. Unlike `select`, a menu holds no bound selection state —
/// its items are one-shot actions. High-level sugar built on the overlay anchor + click-through primitives;
/// lives in `ui-components`, not the kernel.
pub struct MenuProps {
    /// The trigger button's label.
    pub label: &'static str,
    /// The action items, listed in the panel in order.
    pub items: Vec<&'static str>,
    /// Fired with the index of the chosen item when it is picked.
    pub on_select: Option<Box<dyn Fn(u32)>>,
    /// Accent colour (trigger border, hover highlight). `Color::TRANSPARENT` (the default) means "unset" and
    /// falls back to the theme accent. A closure so a theme token re-reads on every render.
    pub color: Box<dyn Fn() -> Color>,
}

impl Default for MenuProps {
    fn default() -> Self {
        Self {
            label: "",
            items: Vec::new(),
            on_select: None,
            color: Box::new(|| Color::TRANSPARENT),
        }
    }
}

// A menu carries no bound selection (`selected: None`), so its rows are one-shot actions: no index is written
// back and no row is highlighted. The label is static, hence the `move || label.to_string()` trigger closure.
pub fn menu(props: MenuProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = props.label;
    dropdown::dropdown(
        move || label.to_string(),
        props.items,
        props.color,
        props.on_select,
        None,
    )
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use ui_core::reset_layout_runtime;

    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerButton, PointerSource};
    use ui_core::{
        ComponentList, EventResult, compute_layout, dispatch_overlays, relayout_if_dirty,
    };

    use super::*;

    fn press(x: f64, y: f64) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }
    fn release(x: f64, y: f64) -> Event {
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }

    // Mirror the runner: consult the overlay registry first, then walk the tree only if no overlay
    // consumed the event (the anchored panel routes through the registry, the trigger through the tree).
    fn route(tree: &mut ComponentList, event: &Event) {
        if dispatch_overlays(event) == EventResult::Ignored {
            tree.on_event(event);
        }
    }

    // Construction: a menu builds headless, lays out (the trigger takes its fixed size), and renders.
    #[test]
    fn builds_and_lays_out() {
        reset_layout_runtime();
        let item = menu(MenuProps {
            label: "Actions",
            items: vec!["Rename", "Duplicate", "Delete"],
            ..Default::default()
        })
        .unwrap();
        let root_node = item.layout_node();
        let root_rect = track_layout(root_node).unwrap();
        compute_layout(
            root_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        assert!(
            root_rect.get().height >= TRIGGER_HEIGHT - 0.5,
            "closed menu is at least the trigger tall: {:?}",
            root_rect.get()
        );
        let tree = ComponentList::new(item);
        let _ = tree.commands();
    }

    // Picking an item fires on_select with its index and closes the menu.
    #[test]
    fn selecting_an_item_fires_on_select_and_closes() {
        reset_layout_runtime();
        let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let item = menu(MenuProps {
            label: "Actions",
            items: vec!["Rename", "Duplicate", "Delete"],
            on_select: Some(Box::new(move |i| sink.set(Some(i)))),
            ..Default::default()
        })
        .unwrap();
        // The widget's own root is the parent-less layout host, laid out at the origin: the trigger sits at
        // (0,0) and the panel anchors directly below it, so click points are computable from the constants.
        let root_node = item.layout_node();
        compute_layout(
            root_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(item);
        let _ = tree.commands();

        // A tap on the trigger (through the tree) opens the panel.
        let tx = (PANEL_WIDTH / 2.0) as f64;
        let ty = (TRIGGER_HEIGHT / 2.0) as f64;
        route(&mut tree, &press(tx, ty));
        route(&mut tree, &release(tx, ty));
        relayout_if_dirty();

        // Item index 1 ("Duplicate") sits at the trigger's bottom + panel padding + one row.
        let ox = (PANEL_WIDTH / 2.0) as f64;
        let oy = (TRIGGER_HEIGHT + PANEL_PAD + ROW_HEIGHT + ROW_HEIGHT / 2.0) as f64;
        route(&mut tree, &press(ox, oy));
        route(&mut tree, &release(ox, oy));

        assert_eq!(
            seen.get(),
            Some(1),
            "picking the second item fires on_select(1)"
        );
        assert_eq!(
            dispatch_overlays(&press(ox, oy)),
            EventResult::Ignored,
            "the menu closes after a pick"
        );
    }
}
