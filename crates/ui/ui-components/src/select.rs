use layout_core::LayoutError;
use reactive_core::{RwSignal, signal};
use renderer_core::Color;
use ui_core::LayoutItem;

use crate::dropdown;
// Re-exported for the test module below, which reads these via `use super::*` to compute click points.
#[cfg(test)]
use crate::dropdown::{PANEL_WIDTH, ROW_HEIGHT, TRIGGER_HEIGHT, panel_pad};
#[cfg(test)]
use ui_core::track_layout;

/// A dropdown bound to a signal: a trigger button showing the currently-selected option, and a click-opened
/// anchored panel listing the options. Picking one writes its index into `selected`, fires `on_select`, and
/// closes. High-level sugar built on the overlay anchor + click-through primitives; lives in `ui-components`,
/// not the kernel, so an app can drop it or ship its own.
pub struct SelectProps {
    /// The bound selection index. `None` (the default) makes the select uncontrolled — it owns an internal
    /// signal so it still tracks a choice, just not one the caller can read.
    pub selected: Option<RwSignal<u32>>,
    /// The option labels; the trigger shows `options[selected]` and the panel lists them in order.
    pub options: Vec<&'static str>,
    /// Accent colour (trigger border, selected/hover highlight). `Color::TRANSPARENT` (the default) means
    /// "unset" and falls back to the theme accent. A closure so a theme token re-reads on every render.
    pub color: Box<dyn Fn() -> Color>,
    /// Fired with the picked index whenever a selection is made.
    pub on_select: Option<Box<dyn Fn(u32)>>,
}

impl Default for SelectProps {
    fn default() -> Self {
        Self {
            selected: None,
            options: Vec::new(),
            color: Box::new(|| Color::TRANSPARENT),
            on_select: None,
        }
    }
}

pub fn select(props: SelectProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // `None` selection is uncontrolled: own an internal signal so the trigger still tracks a choice.
    let selected = props.selected.unwrap_or_else(|| signal(0u32));
    let options = props.options;
    // The trigger's label reactively tracks the selected option (a menu's is static); `Some(selected)` tells
    // the dropdown to write the picked index back and highlight the selected row.
    let trigger_label = {
        let selected = selected.clone();
        let options = options.clone();
        move || {
            options
                .get(selected.get() as usize)
                .copied()
                .unwrap_or("Select")
                .to_string()
        }
    };
    dropdown::dropdown(
        trigger_label,
        options,
        props.color,
        props.on_select,
        Some(selected),
    )
}

#[cfg(test)]
mod tests {
    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerButton, PointerSource};
    use reactive_core::signal;
    use ui_core::reset_layout_runtime;
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

    // Construction: a select builds headless, lays out (the trigger takes its fixed size), and renders.
    #[test]
    fn builds_and_lays_out() {
        reset_layout_runtime();
        let picked = signal(1u32);
        let item = select(SelectProps {
            selected: Some(picked.clone()),
            options: vec!["Small", "Medium", "Large"],
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
            "closed select is at least the trigger tall: {:?}",
            root_rect.get()
        );
        // A ComponentList renders it without panicking (closed: no overlay in the tree).
        let tree = ComponentList::new(item);
        let _ = tree.commands();
    }

    // Selecting an option writes its index into the bound signal and fires on_select, then closes.
    #[test]
    fn selecting_an_option_sets_the_signal_and_closes() {
        use std::cell::Cell;
        use std::rc::Rc;

        reset_layout_runtime();
        let picked = signal(0u32);
        let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let item = select(SelectProps {
            selected: Some(picked.clone()),
            options: vec!["Small", "Medium", "Large"],
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

        // A tap outside any overlay hits the trigger through the tree and toggles the panel open.
        let tx = (PANEL_WIDTH / 2.0) as f64;
        let ty = (TRIGGER_HEIGHT / 2.0) as f64;
        route(&mut tree, &press(tx, ty));
        route(&mut tree, &release(tx, ty));
        // Opening built + portaled the overlay; relayout lays the panel out so its barrier is live.
        relayout_if_dirty();

        // Option index 2 ("Large") sits at the trigger's bottom + panel padding + two rows.
        let ox = PANEL_WIDTH / 2.0;
        let oy = (TRIGGER_HEIGHT + panel_pad() + 2.0 * ROW_HEIGHT + ROW_HEIGHT / 2.0) as f64;
        route(&mut tree, &press(ox as f64, oy));
        route(&mut tree, &release(ox as f64, oy));

        assert_eq!(
            picked.get(),
            2,
            "picking the third option sets the signal to 2"
        );
        assert_eq!(seen.get(), Some(2), "on_select fires with the picked index");
        // Selecting closed the panel, so its barrier no longer intercepts a tap where it used to be.
        assert_eq!(
            dispatch_overlays(&press(ox as f64, oy)),
            EventResult::Ignored,
            "the panel closes after a selection"
        );
    }
}
