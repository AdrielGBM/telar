//! [`menu`]: a button that opens a list of one-shot actions.

use std::rc::Rc;

use layout_core::LayoutError;
use reactive_core::Reactive;
use renderer_core::Color;
use telar_macros::Props;
#[cfg(test)]
use ui_core::Slots;
use ui_core::{Children, LayoutItem};

use crate::dropdown;
// Re-exported for the test module below, which reads them via `use super::*` to compute click points.
#[cfg(test)]
use crate::dropdown::{PANEL_WIDTH, ROW_HEIGHT, TRIGGER_HEIGHT, panel_pad};
#[cfg(test)]
use ui_core::track_layout;

/// A click-triggered list of action items: a labelled trigger button that opens an anchored list; picking an item fires `on_select` with its index and closes. Unlike `select`, a menu holds no bound selection state — its items are one-shot actions. High-level sugar built on the overlay anchor + click-through primitives; lives in `ui-components`, not the kernel.
#[derive(Props)]
pub struct MenuProps {
    /// The trigger button's label.
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Fired with the index of the chosen item when it is picked.
    #[props(some, default)]
    pub on_select: Option<Rc<dyn Fn(u32)>>,
    /// Accent colour (trigger border, hover highlight). `Color::TRANSPARENT` (the default) means "unset" and falls back to the theme accent. A closure so a theme token re-reads on every render.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Take the width the row offers instead of the fixed trigger width — see [`crate::SelectProps::stretch`].
    #[props(default)]
    pub stretch: bool,
    /// Draw the trigger as a field, with the border a `select` carries. Off by default, because a menu is a *button* that happens to open a list, and a button wears no frame until it is pressed.
    ///
    /// A prop and not a decision settled inside the component, because both readings are legitimate: a menu standing alone in a header wants no frame, and one sitting in a row of fields wants to match them.
    #[props(default)]
    pub bordered: bool,
    /// Show the caret that says the trigger opens something. On by default.
    #[props(default = true)]
    pub caret: bool,
    /// Amends the paint of the trigger — this component's **principal surface**, the thing a caller means when they point at a menu. See `shared::SurfaceStyle` for why it takes the finished style rather than naming one property, and for when a theme token is the right instrument instead.
    #[props(some, default)]
    pub style: Option<Rc<dyn Fn(renderer_core::RectStyle) -> renderer_core::RectStyle>>,
}

// A menu carries no bound selection, so its rows are one-shot actions: no index is written back and no row is highlighted.
/// A button that opens a list of one-shot actions.
pub fn menu(props: MenuProps, children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    dropdown::dropdown(dropdown::Dropdown {
        label: dropdown::TriggerLabel::Fixed(props.label),
        rows: children,
        color: props.color,
        on_pick: props.on_select,
        selected: None,
        stretch: props.stretch,
        bordered: props.bordered,
        caret: props.caret,
        style: props.style,
    })
}

/// A plain row per label, which is what the markup `menu … item label:"…"` compiles to. Enough for the tests that care about the trigger and the panel rather than about what a row can be.
#[cfg(test)]
fn rows(labels: &'static [&'static str]) -> Children {
    Children::new(move || {
        let mut slots = Slots::new();
        for label in labels {
            slots.push(
                None,
                crate::list::item(
                    crate::list::ItemProps::props()
                        .label(Reactive::of(move || label.to_string()))
                        .build(),
                    Children::default(),
                )?,
            );
        }
        Ok(slots)
    })
}

/// The shape of a trigger is the caller's call, not the component's.
///
/// The default splits them by what they are — a menu is a button, a select is a field — but a menu dropped into a row of inputs wants to match them, and there has to be a way to say so that is not editing the catalogue. Guards the props rather than the pixels: what matters is that they *reach* the trigger's paint at all.
#[cfg(test)]
#[test]
fn a_menu_can_be_asked_for_a_field_and_for_no_caret() {
    use renderer_core::DrawCommand;
    use ui_core::{ComponentList, LayoutItem};

    let strokes = |bordered: bool, caret: bool| {
        crate::test_support::fresh_layout_runtime();
        let item = menu(
            MenuProps::props()
                .label("File")
                .bordered(bordered)
                .caret(caret)
                .build(),
            rows(&["New"]),
        )
        .unwrap();
        // Laid out first: the caret is drawn by a `Canvas`, which has nothing to draw into until it has a rect.
        let root = ui_core::new_container(
            layout_core::LayoutStyle::new().width(300.0).height(80.0),
            &[item.layout_node()],
        )
        .unwrap();
        ui_core::compute_layout(
            root,
            layout_core::AvailableSpace::Definite(300.0),
            layout_core::AvailableSpace::Definite(80.0),
        )
        .unwrap();
        let tree = ComponentList::new(item);
        let cmds = tree.commands().to_vec();
        let bordered_boxes = cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::Rect { style, .. } if style.border.is_some()))
            .count();
        let paths = cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::Path { .. }))
            .count();
        (bordered_boxes, paths)
    };

    let (plain_border, plain_caret) = strokes(false, true);
    assert_eq!(
        plain_border, 0,
        "a menu is a button, so no frame by default"
    );
    assert_eq!(plain_caret, 1, "and it says it opens something");

    let (asked_border, _) = strokes(true, true);
    assert_eq!(asked_border, 1, "a caller that wants the field gets it");

    let (_, no_caret) = strokes(false, false);
    assert_eq!(no_caret, 0, "and one that wants no caret gets none");
}

/// A caller can restyle the surface without editing the catalogue.
///
/// The pressure this relieves is real: an app wanted its menu trigger squared off, and the only lever the catalogue offered was the theme's radius — which moves every rounded thing in the application. Editing `dropdown.rs` to hold one app's opinion is how a shared component stops being shared.
///
/// The amendment must *compose*, not replace: the component still decides that a bordered trigger wears a stroke, and the caller only says what they came to say.
#[cfg(test)]
#[test]
fn a_caller_can_amend_the_paint_the_trigger_worked_out_for_itself() {
    use renderer_core::{BorderRadius, DrawCommand, ShapeStyle};
    use ui_core::{ComponentList, LayoutItem};

    crate::test_support::fresh_layout_runtime();
    let item = menu(
        MenuProps::props()
            .label("File")
            .bordered(true)
            .style(Rc::new(|s| {
                s.with_radius(BorderRadius::all(0.0))
                    .with_fill(Color::rgba(1.0, 0.0, 0.0, 1.0))
            }))
            .build(),
        rows(&["New"]),
    )
    .unwrap();
    let root = ui_core::new_container(
        layout_core::LayoutStyle::new().width(300.0).height(80.0),
        &[item.layout_node()],
    )
    .unwrap();
    ui_core::compute_layout(
        root,
        layout_core::AvailableSpace::Definite(300.0),
        layout_core::AvailableSpace::Definite(80.0),
    )
    .unwrap();
    let tree = ComponentList::new(item);
    let trigger = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Rect { style, .. } if style.border.is_some() => Some(style.clone()),
            _ => None,
        })
        .expect("a bordered trigger is painted");

    assert_eq!(
        trigger.radius,
        BorderRadius::all(0.0),
        "the caller's radius reaches the paint"
    );
    assert_eq!(
        trigger.fill,
        Some(renderer_core::Paint::Solid(Color::rgba(1.0, 0.0, 0.0, 1.0))),
        "and so does their fill"
    );
    assert!(
        trigger.border.is_some(),
        "while the component keeps the border it decided a field wears"
    );
}

#[cfg(test)]
#[path = "menu_test.rs"]
mod tests;
