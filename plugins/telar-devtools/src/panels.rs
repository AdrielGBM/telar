//! The inspection panels the overlay and the workshop both mount over an [`Inspection`]: the component inspector with its box model, the accessibility tree with what the checks found, the toggles for the outline and the measure, and the marks they draw over what is inspected.
//!
//! Each is built in the workbench theme from `telar-components` widgets and reads nothing but the [`Inspection`] it is given, so a host places them wherever its chrome has room.

mod accessibility;
mod inspector;
mod marks;

pub use accessibility::accessibility_panel;
pub use inspector::inspector_panel;
pub use marks::inspection_marks;

pub(crate) use accessibility::{READER_ROW, VIOLATIONS_ROW};

use telar::{
    AlignItems, Children, Container, LayoutError, LayoutItem, LayoutStyle, Reactive, ReactiveList,
    RwSignal, Slots, box_item, on_cleanup,
};
use telar_components::{ToggleProps, toggle};

use crate::inspect::Inspection;
use crate::strings::{self, MEASURE, OUTLINE};
use crate::workbench::WORKBENCH_GRID;

/// The switches for the marks drawn over what is inspected: every component's box, and the selected one's size and distances.
pub fn inspection_toggles(inspection: Inspection) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let switches = vec![
        switch(inspection.outline, OUTLINE)?,
        switch(inspection.measure, MEASURE)?,
    ];
    Ok(box_item(Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID * 2.0),
        switches,
    )?))
}

fn switch(on: RwSignal<bool>, label: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    toggle(
        ToggleProps::props()
            .checked(on)
            .label(Reactive::of(move || strings::text(label)))
            .build(),
        Children::default(),
    )
}

/// Counts a panel in `mounted` for as long as the scope building it lives.
fn count_mounted(mounted: RwSignal<u32>) {
    mounted.update(|count| *count += 1);
    on_cleanup(move || mounted.update(|count| *count = count.saturating_sub(1)));
}

/// A layer mounted only while `shown` says so, positioned by `style`.
pub(crate) fn mounted_while(
    style: LayoutStyle,
    shown: impl Fn() -> bool + 'static,
    build: impl Fn() -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let layer = ReactiveList::with_style(
        style,
        move || if shown() { vec![()] } else { Vec::new() },
        |_: &()| (),
        move |()| build(),
    )?;
    Ok(box_item(layer))
}

pub(crate) fn panes(items: [Box<dyn LayoutItem>; 2]) -> Children {
    let mut slots = Slots::new();
    for item in items {
        slots.push(None, item);
    }
    Children::from(slots)
}

/// A number as a measurement reads: whole where it is whole, one decimal where it is not.
pub(crate) fn number(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

#[cfg(test)]
#[path = "panels_test.rs"]
mod tests;
