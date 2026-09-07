//! [`select`]: a field that opens a list and writes the picked index back.

use std::rc::Rc;

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

/// A dropdown bound to a signal: a trigger button showing the currently-selected option, and a click-opened anchored panel listing the choices. Picking one writes its index into `selected`, fires `on_select`, and closes. High-level sugar built on the overlay anchor + click-through primitives; lives in `ui-components`, not the kernel, so an app can drop it or ship its own.
///
/// Its choices are written as `item` children, the same pieces a `menu` is made of, so one can be disabled or carry an icon — which a list of strings could never say. What made that impossible for a select and not for a menu was the trigger: it has to name the current choice before the panel has ever been opened, and the rows only exist once it has. See `ListContext::declare`.
#[derive(Props)]
pub struct SelectProps {
    /// The bound selection index. `None` (the default) makes the select uncontrolled — it owns an internal signal so it still tracks a choice, just not one the caller can read.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<u32>>,
    /// Accent colour (trigger border, selected/hover highlight). `Color::TRANSPARENT` (the default) means "unset" and falls back to the theme accent. A closure so a theme token re-reads on every render.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Fired with the picked index whenever a selection is made.
    #[props(some, default)]
    pub on_select: Option<Rc<dyn Fn(u32)>>,
    /// Take the width the row offers instead of the fixed trigger width — what a form field wants, where a 180px control beside full-width ones reads as a mistake. The panel opens at that width too.
    #[props(default = false)]
    pub stretch: bool,
}

/// A field that opens a list and writes the picked index back.
pub fn select(props: SelectProps, children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let rows = children;
    // `None` selection is uncontrolled: own an internal signal so the trigger still tracks a choice.
    let selected = props.selected.unwrap_or_else(|| signal(0u32));
    dropdown::dropdown(dropdown::Dropdown {
        style: None,
        // Also tells the dropdown to write the picked index back and highlight the chosen row, which is the rest of what makes this a bound list rather than a menu.
        label: dropdown::TriggerLabel::Selected {
            placeholder: "Select",
        },
        rows,
        color: props.color,
        on_pick: props.on_select,
        selected: Some(selected),
        stretch: props.stretch,
        // A select is a field: it wears the border and the caret, and a bare one is indistinguishable from a label.
        bordered: true,
        caret: true,
    })
}

#[cfg(test)]
#[path = "select_test.rs"]
mod tests;
