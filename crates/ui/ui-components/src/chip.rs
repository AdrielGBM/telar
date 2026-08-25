use std::rc::Rc;

use layout_core::{AlignItems, LayoutError, LayoutStyle};
use renderer_core::{Border, BorderRadius, Color, RectStyle, ShapeStyle};
use ui_core::focus::Role;
use ui_core::{Container, LayoutItem, StyledContainer, Text, box_item};

use crate::shared;
use crate::shared::props_default;

fn pad_x() -> f32 {
    shared::spacing() * 1.25
}
fn pad_y() -> f32 {
    shared::spacing() * 0.5
}
fn radius() -> f32 {
    shared::radius() * 3.0
}
fn gap() -> f32 {
    shared::spacing() * 0.75
}
/// A chip's label and its close glyph, as shares of the text around them.
const TEXT_RATIO: f32 = 0.93;
const CLOSE_RATIO: f32 = 0.85;
fn dot_size() -> f32 {
    shared::spacing() * 0.75
}

fn dot_box() -> LayoutStyle {
    LayoutStyle::new().width(dot_size()).height(dot_size())
}
fn inner_row() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .gap(gap())
}
fn pill_box() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .padding_horizontal(pad_x())
        .padding_vertical(pad_y())
}

/// A small outlined tag, quieter than `badge`'s solid fill: a bordered surface pill with normal ink text,
/// an optional small accent dot when `color` is set, and an optional `×` affordance that fires `on_close`.
/// Non-interactive unless `on_close` is set. High-level sugar over `StyledContainer`/`Container` + `Text`;
/// lives in `ui-components`, not the kernel, so an app can drop it or ship its own.
pub struct ChipProps {
    pub label: Box<dyn Fn() -> String>,
    /// Small leading accent dot colour. `Color::TRANSPARENT` (the default) means "unset": no dot is shown at
    /// all (not just an invisible one) — see `chip`'s doc. A closure (re-read every frame) so a theme token
    /// or `$signal` colour re-colours the dot live, like `button`'s `fill`.
    pub color: Box<dyn Fn() -> Color>,
    /// When `Some`, a small `×` press target renders on the right and calls it on tap. `None` (the default)
    /// omits it entirely, leaving a non-interactive chip.
    pub on_close: Option<Box<dyn Fn()>>,
}

props_default!(ChipProps {
    label: text,
    color: color,
    on_close: none,
});

pub fn chip(props: ChipProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ChipProps {
        label,
        color,
        on_close,
    } = props;
    // Erased to `Rc` so the same colour closure can feed both the dot's presence check (read once, below)
    // and its per-frame fill style, like `button`'s `fill`/`outline`.
    let color: shared::ReactiveColor = Rc::from(color);

    let mut children: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(3);

    // The dot's presence is decided once at build time (an unset colour means "no dot", not an invisible
    // one); its shade is still re-read every frame so a live theme/signal colour tracks.
    if (color.as_ref())() != Color::TRANSPARENT {
        let dot_color = Rc::clone(&color);
        let dot = StyledContainer::new(dot_box(), move |_r| dot_style(dot_color.as_ref()), vec![])?
            .styled_by(dot_box);
        children.push(box_item(dot));
    }

    let label_widget = Text::declaring(
        move || label(),
        LayoutStyle::new(),
        |t| shared::control_text(t, TEXT_RATIO),
    )?;
    children.push(box_item(label_widget));

    if let Some(cb) = on_close {
        let close_label = Text::declaring(
            || "×".to_string(),
            LayoutStyle::new(),
            |t| shared::control_text(t, CLOSE_RATIO),
        )?;
        let close = StyledContainer::new(
            LayoutStyle::new().flex_row(),
            |_r| RectStyle::default(),
            vec![box_item(close_label)],
        )?
        .control(Role::Button)
        .on_press(cb);
        children.push(box_item(close));
    }

    let row = Container::new(inner_row(), children)?.styled_by(inner_row);

    let pill = StyledContainer::new(
        pill_box(),
        |_r| {
            RectStyle::default()
                .with_fill(shared::surface_alt())
                .with_border(Border::uniform(shared::border(), 1.0))
                .with_radius(BorderRadius::all(radius()))
        },
        vec![box_item(row)],
    )?
    .styled_by(pill_box);
    Ok(box_item(pill))
}

fn dot_style(color: &dyn Fn() -> Color) -> RectStyle {
    RectStyle::default()
        .with_fill(color())
        .with_radius(BorderRadius::all(dot_size() / 2.0))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerButton, PointerSource};
    use renderer_core::DrawCommand;
    use ui_core::{ComponentList, compute_layout, new_container, track_layout};

    use super::*;

    fn find_text(cmds: &[DrawCommand], needle: &str) -> bool {
        cmds.iter()
            .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.as_ref() == needle))
    }

    // A labelled chip draws its label text.
    #[test]
    fn renders_label() {
        crate::test_support::fresh_layout_runtime();
        let chip = chip(ChipProps {
            label: Box::new(|| "Draft".to_string()),
            ..Default::default()
        })
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_row().width(200.0).height(60.0),
            &[chip.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(60.0),
        )
        .unwrap();
        let tree = ComponentList::new(chip);
        assert!(find_text(&tree.commands(), "Draft"));
    }

    // Building with `on_close: Some(...)` works and renders the × affordance; tapping it fires the callback.
    #[test]
    fn on_close_renders_and_fires_on_tap() {
        crate::test_support::fresh_layout_runtime();
        let flag = Rc::new(Cell::new(false));
        let sink = flag.clone();
        let mut chip = chip(ChipProps {
            label: Box::new(|| "Tag".to_string()),
            on_close: Some(Box::new(move || sink.set(true))),
            ..Default::default()
        })
        .unwrap();
        let node = chip.layout_node();
        // Wrapped in a non-stretching root (like `checkbox`'s test helper) so the pill shrink-wraps to its
        // content instead of filling the root's full width — the earlier edge-tap math otherwise missed
        // the × entirely, landing in dead space to the right of a stretched pill.
        let rect = track_layout(node).unwrap();
        let root = new_container(
            LayoutStyle::new().flex_row().width(200.0).height(60.0),
            &[node],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(60.0),
        )
        .unwrap();

        let r = rect.get();
        // Tap just inside the pill's right padding, where the × close target sits.
        let (cx, cy) = (
            (r.x + r.width - pad_x() - 3.0) as f64,
            (r.y + r.height / 2.0) as f64,
        );
        chip.on_event(&Event::PointerPressed {
            x: cx,
            y: cy,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        chip.on_event(&Event::PointerReleased {
            x: cx,
            y: cy,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        assert!(flag.get(), "tapping the × fires on_close");
    }

    // An empty label still builds and lays out without panicking.
    #[test]
    fn empty_label_builds_without_panic() {
        crate::test_support::fresh_layout_runtime();
        let chip = chip(ChipProps::default()).unwrap();
        let root = new_container(
            LayoutStyle::new().flex_row().width(200.0).height(60.0),
            &[chip.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(60.0),
        )
        .unwrap();
        let tree = ComponentList::new(chip);
        let _ = tree.commands();
    }
}
