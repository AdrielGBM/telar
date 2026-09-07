//! [`tabs`]: a row of pills selecting one index.

use telar_macros::Props;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::{Reactive, RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, TextStyle};
use ui_core::focus::Role;
use ui_core::{Children, Container, LayoutItem, StyledContainer, Text, box_item};

use crate::shared;

fn pad_x() -> f32 {
    shared::spacing() * 2.0
}
fn pad_y() -> f32 {
    shared::spacing()
}
fn gap() -> f32 {
    shared::spacing() * 0.5
}
fn radius() -> f32 {
    shared::radius() * 1.5
}
fn tab_box() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_horizontal(pad_x())
        .padding_vertical(pad_y())
}
fn bar() -> LayoutStyle {
    LayoutStyle::new().flex_row().gap(gap())
}

/// A horizontal tab bar: one button per label, driving a `selected` index. Renders only the row of tab buttons — the matching content panel is the caller's responsibility (typically the DSL's reactive `if selected == i`, mirroring the sandbox's own nav-button/section-switch split). Modelled on `select.rs`'s items/selected handling, but rendered inline (a row) rather than as an anchored overlay. High-level sugar over the primitives; lives in `ui-components`, not the kernel.
#[derive(Props)]
pub struct TabsProps {
    /// The tab labels, rendered in order.
    #[props(default)]
    pub items: Vec<&'static str>,
    /// Bound active index. `None` (the default) is uncontrolled — the widget owns its own `signal(0)`.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<u32>>,
    /// Accent (active tab fill). `Color::TRANSPARENT` (the default) means "unset": falls back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
}

/// A row of pills selecting one index.
pub fn tabs(props: TabsProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let TabsProps {
        items,
        selected,
        color,
    } = props;
    // Uncontrolled: own the index so the bar still tracks the active tab when the caller binds no signal.
    let selected = selected.unwrap_or_else(|| signal(0u32));
    let mut tab_items: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(items.len());
    for (i, label) in items.into_iter().enumerate() {
        let idx = i as u32;
        let label_selected = selected;
        let label_widget = Text::declaring(
            move || label.to_string(),
            LayoutStyle::new(),
            move |t| tab_text(t, label_selected.get() == idx),
        )?;

        let base_selected = selected;
        let base_color = color.clone();
        let hover_selected = selected;
        let hover_color = color.clone();
        let announced_selected = selected;
        let press_selected = selected;
        let tab = StyledContainer::new(
            tab_box(),
            move |_r| tab_rect(base_selected.get() == idx, &base_color, false),
            vec![box_item(label_widget)],
        )?
        .styled_by(tab_box)
        .hover_style(move |_r| tab_rect(hover_selected.get() == idx, &hover_color, true))
        .control(Role::Tab)
        .toggled(move || announced_selected.get() == idx)
        .on_press(move || press_selected.set(idx));
        tab_items.push(box_item(tab));
    }

    let row = Container::new(bar(), tab_items)?.styled_by(bar);
    Ok(box_item(row))
}

/// The tab pill's paint: the active tab fills with the accent (a touch darker on hover); an inactive tab blends in until hovered, when it lifts to a faint accent wash.
fn tab_rect(active: bool, color: &Reactive<Color>, hovered: bool) -> RectStyle {
    let radius = BorderRadius::all(radius());
    let accent = shared::resolve(color, shared::accent);
    if active {
        let fill = if hovered { accent.darken(0.15) } else { accent };
        return RectStyle::default().with_fill(fill).with_radius(radius);
    }
    if hovered {
        return RectStyle::default()
            .with_fill(accent.with_alpha(0.10))
            .with_radius(radius);
    }
    RectStyle::default().with_radius(radius)
}

/// A tab's label: legible on the accent pill when it is the selected one, and otherwise a quieter shade of whatever the bar around it is written in.
///
/// The inactive tone was a flat grey literal — the same grey in dark mode, and the same grey under a region that had declared its own ink. It was also, to two decimals, what fading the default ink over a white page produces: a light-mode screenshot of a value the cascade already knows how to work out.
fn tab_text(inherited: TextStyle, active: bool) -> TextStyle {
    if active {
        shared::control_text(inherited, 1.0).with_color(shared::on_accent())
    } else {
        shared::quiet(inherited, 1.0)
    }
}

#[cfg(test)]
#[path = "tabs_test.rs"]
mod tests;
