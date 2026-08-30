use layout_core::LayoutError;
use reactive_core::{Reactive, RwSignal, signal};
use renderer_core::Color;
use telar_macros::Props;
#[cfg(test)]
use ui_core::Slots;
use ui_core::{Children, LayoutItem};

use crate::dropdown;
// Re-exported for the test module below, which reads these via `use super::*` to compute click points.
#[cfg(test)]
use crate::dropdown::{PANEL_WIDTH, ROW_HEIGHT, TRIGGER_HEIGHT, panel_pad};
#[cfg(test)]
use ui_core::track_layout;

/// A dropdown bound to a signal: a trigger button showing the currently-selected option, and a click-opened
/// anchored panel listing the choices. Picking one writes its index into `selected`, fires `on_select`, and
/// closes. High-level sugar built on the overlay anchor + click-through primitives; lives in `ui-components`,
/// not the kernel, so an app can drop it or ship its own.
///
/// Its choices are written as `item` children, the same pieces a `menu` is made of, so one can be disabled or
/// carry an icon — which a list of strings could never say. What made that impossible for a select and not for
/// a menu was the trigger: it has to name the current choice before the panel has ever been opened, and the
/// rows only exist once it has. See [`ListContext::declare`](crate::list::ListContext).
#[derive(Props)]
pub struct SelectProps {
    /// The bound selection index. `None` (the default) makes the select uncontrolled — it owns an internal
    /// signal so it still tracks a choice, just not one the caller can read.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<u32>>,
    /// Accent colour (trigger border, selected/hover highlight). `Color::TRANSPARENT` (the default) means
    /// "unset" and falls back to the theme accent. A closure so a theme token re-reads on every render.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Fired with the picked index whenever a selection is made.
    #[props(some, default)]
    pub on_select: Option<Box<dyn Fn(u32)>>,
    /// Take the width the row offers instead of the fixed trigger width — what a form field wants, where a
    /// 180px control beside full-width ones reads as a mistake. The panel opens at that width too.
    #[props(default = false)]
    pub stretch: bool,
}

pub fn select(props: SelectProps, rows: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // `None` selection is uncontrolled: own an internal signal so the trigger still tracks a choice.
    let selected = props.selected.unwrap_or_else(|| signal(0u32));
    dropdown::dropdown(dropdown::Dropdown {
        style: None,
        // `Some(selected)` below also tells the dropdown to write the picked index back and highlight the
        // chosen row, which is the rest of what makes this a bound list rather than a menu.
        label: dropdown::TriggerLabel::Selected {
            placeholder: "Select",
        },
        rows,
        color: props.color,
        on_pick: props.on_select,
        selected: Some(selected),
        stretch: props.stretch,
        // A select *is* a field: it wears the border and the caret, and neither is up for discussion the way
        // a menu's are — a bare select is indistinguishable from a label.
        bordered: true,
        caret: true,
    })
}

#[cfg(test)]
mod tests {
    use layout_core::AvailableSpace;

    use reactive_core::signal;
    use ui_core::{
        ComponentList, EventResult, compute_layout, dispatch_overlays, relayout_if_dirty,
    };

    use super::*;
    use crate::harness::{press, release, route};

    /// Three choices as `item` rows, which is how a select is written now.
    fn sizes() -> Children {
        Children::new(|| {
            let mut slots = Slots::new();
            for label in ["Small", "Medium", "Large"] {
                slots.push(
                    None,
                    crate::list::item(
                        crate::list::ItemProps::props()
                            .label(Reactive::of(move || label.to_string()))
                            .build(),
                        Slots::new(),
                    )?,
                );
            }
            Ok(slots)
        })
    }

    // Mirror the runner: consult the overlay registry first, then walk the tree only if no overlay
    // consumed the event (the anchored panel routes through the registry, the trigger through the tree).

    // Construction: a select builds headless, lays out (the trigger takes its fixed size), and renders.
    #[test]
    fn builds_and_lays_out() {
        crate::test_support::fresh_layout_runtime();
        let picked = signal(1u32);
        let item = select(SelectProps::props().selected(picked).build(), sizes()).unwrap();
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

        crate::test_support::fresh_layout_runtime();
        let picked = signal(0u32);
        let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let item = select(
            SelectProps::props()
                .selected(picked)
                .on_select(Box::new(move |i| sink.set(Some(i))))
                .build(),
            sizes(),
        )
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

    /// The constraint that kept a select on a flat `options` prop, met head on: the trigger says the chosen
    /// row's own label *before the panel has ever been opened*, which is the only moment where there are no
    /// rows to read it from. The declaring walk is what supplies it.
    #[test]
    fn the_trigger_names_the_chosen_row_before_the_panel_has_ever_opened() {
        crate::test_support::fresh_layout_runtime();
        let picked = signal(1u32);
        let item = select(SelectProps::props().selected(picked).build(), sizes()).unwrap();
        compute_layout(
            item.layout_node(),
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let tree = ComponentList::new(item);

        assert!(
            drawn_text(&tree).iter().any(|t| t == "Medium"),
            "the trigger reads the label off row 1 without the panel existing: {:?}",
            drawn_text(&tree)
        );
    }

    /// And what the flat prop could never say. A row is an ordinary component with ordinary props, so one of
    /// them can be disabled — and a disabled row is not a place the keyboard stops or a tap commits.
    #[test]
    fn a_choice_can_be_disabled_which_a_list_of_strings_could_not_say() {
        use std::cell::Cell;
        use std::rc::Rc;

        crate::test_support::fresh_layout_runtime();
        let picked = signal(0u32);
        let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let rows = Children::new(|| {
            let mut slots = Slots::new();
            for (label, disabled) in [("Small", false), ("Medium", true), ("Large", false)] {
                slots.push(
                    None,
                    crate::list::item(
                        crate::list::ItemProps::props()
                            .label(Reactive::of(move || label.to_string()))
                            .disabled(Reactive::of(move || disabled))
                            .build(),
                        Slots::new(),
                    )?,
                );
            }
            Ok(slots)
        });
        let item = select(
            SelectProps::props()
                .selected(picked)
                .on_select(Box::new(move |i| sink.set(Some(i))))
                .build(),
            rows,
        )
        .unwrap();
        compute_layout(
            item.layout_node(),
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(item);
        let _ = tree.commands();

        let tx = (PANEL_WIDTH / 2.0) as f64;
        route(&mut tree, &press(tx, (TRIGGER_HEIGHT / 2.0) as f64));
        route(&mut tree, &release(tx, (TRIGGER_HEIGHT / 2.0) as f64));
        relayout_if_dirty();

        // The middle row, which is the disabled one.
        let oy = (TRIGGER_HEIGHT + panel_pad() + ROW_HEIGHT + ROW_HEIGHT / 2.0) as f64;
        route(&mut tree, &press(tx, oy));
        route(&mut tree, &release(tx, oy));

        assert_eq!(seen.get(), None, "a disabled choice commits nothing");
        assert_eq!(picked.get(), 0, "and leaves the bound signal alone");
    }

    /// Every string the tree draws, for asserting on what a trigger says.
    fn drawn_text(tree: &ComponentList) -> Vec<String> {
        tree.commands()
            .iter()
            .filter_map(|c| match c {
                renderer_core::DrawCommand::Text { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .collect()
    }
}
