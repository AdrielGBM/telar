use std::rc::Rc;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, TextStyle};
use theme_core::use_theme_tokens;
use ui_core::{Container, LayoutItem, StyledContainer, Text, box_item};

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
fn font_size() -> f32 {
    shared::font_size()
}
fn radius() -> f32 {
    shared::radius() * 1.5
}
const INK_MUTED: Color = Color::rgba(0.45, 0.45, 0.52, 1.0);

/// A horizontal tab bar: one button per label, driving a `selected` index. Renders only the row of tab
/// buttons — the matching content panel is the caller's responsibility (typically the DSL's reactive
/// `if selected == i`, mirroring the sandbox's own nav-button/section-switch split). Modelled on
/// `select.rs`'s items/selected handling, but rendered inline (a row) rather than as an anchored overlay.
/// High-level sugar over the primitives; lives in `ui-components`, not the kernel.
pub struct TabsProps {
    /// The tab labels, rendered in order.
    pub items: Vec<&'static str>,
    /// Bound active index. `None` (the default) is uncontrolled — the widget owns its own `signal(0)`.
    pub selected: Option<RwSignal<u32>>,
    /// Accent (active tab fill). `Color::TRANSPARENT` (the default) means "unset": falls back to the theme accent.
    pub color: Box<dyn Fn() -> Color>,
}

impl Default for TabsProps {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            selected: None,
            color: Box::new(|| Color::TRANSPARENT),
        }
    }
}

pub fn tabs(props: TabsProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let TabsProps {
        items,
        selected,
        color,
    } = props;
    // Uncontrolled: own the index so the bar still tracks the active tab when the caller binds no signal.
    let selected = selected.unwrap_or_else(|| signal(0u32));
    // Erased to `Rc` so the same colour closure feeds every tab's fill, hover and label style.
    let color: shared::ReactiveColor = Rc::from(color);

    let mut tab_items: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(items.len());
    for (i, label) in items.into_iter().enumerate() {
        let idx = i as u32;
        let label_selected = selected.clone();
        let label_widget = Text::auto(
            move || label.to_string(),
            LayoutStyle::new(),
            move || TextStyle::new(font_size(), tab_ink(label_selected.get() == idx)),
        )?;

        let base_selected = selected.clone();
        let base_color = color.clone();
        let hover_selected = selected.clone();
        let hover_color = color.clone();
        let press_selected = selected.clone();
        let tab = StyledContainer::new(
            LayoutStyle::new()
                .flex_row()
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::CENTER)
                .padding_horizontal(pad_x())
                .padding_vertical(pad_y()),
            move |_r| tab_rect(base_selected.get() == idx, base_color.as_ref(), false),
            vec![box_item(label_widget)],
        )?
        .on_hover_style(move |_r| tab_rect(hover_selected.get() == idx, hover_color.as_ref(), true))
        .on_press(move || press_selected.set(idx));
        tab_items.push(box_item(tab));
    }

    let row = Container::new(LayoutStyle::new().flex_row().gap(gap()), tab_items)?;
    Ok(box_item(row))
}

/// The tab pill's paint: the active tab fills with the accent (a touch darker on hover); an inactive tab
/// blends in until hovered, when it lifts to a faint accent wash.
fn tab_rect(active: bool, color: &dyn Fn() -> Color, hovered: bool) -> RectStyle {
    let radius = BorderRadius::all(radius());
    let accent = shared::resolve(color, || {
        use_theme_tokens()
            .map(|t| t.primary())
            .unwrap_or(shared::DEFAULT_ACCENT)
    });
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

/// The label ink: white on the active (filled) tab for contrast, muted on the rest.
fn tab_ink(active: bool) -> Color {
    if active {
        use_theme_tokens()
            .map(|t| t.on_primary())
            .unwrap_or(Color::WHITE)
    } else {
        INK_MUTED
    }
}

#[cfg(test)]
mod tests {
    use ui_core::reset_layout_runtime;

    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerButton, PointerSource};
    use ui_core::{Component, NodeId, compute_layout, new_container, track_layout};

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

    // Lays `node` out as the sole child of a 400×100 root (an auto-size ROOT fills its available space, so
    // laying `node` itself out as the root would force it to 400px wide instead of its natural content
    // width) and returns its laid-out rect.
    fn lay_out(node: NodeId) -> geometry_core::Rect {
        let rect = track_layout(node).unwrap();
        let root = new_container(
            LayoutStyle::new().flex_row().width(400.0).height(100.0),
            &[node],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        rect.get()
    }

    // Construction: a tab bar builds headless, lays out (a row of measured tab pills), and renders.
    #[test]
    fn builds_and_lays_out() {
        reset_layout_runtime();
        let item = tabs(TabsProps {
            items: vec!["One", "Two", "Three"],
            ..Default::default()
        })
        .unwrap();
        let r = lay_out(item.layout_node());
        assert!(r.width > 0.0, "the tab row takes some width");
        let _ = item.view();
    }

    // Pressing the last tab sets the bound `selected` signal to its index. With exactly two (content-sized,
    // `Text::auto`) tabs and no `flex_grow`/stretch on the row, its right edge sits flush against the second
    // tab's own padding, so a point just inside that edge deterministically lands on index 1 — without
    // hardcoding font metrics to compute the boundary between the two pills.
    #[test]
    fn pressing_the_last_tab_sets_selected_index() {
        reset_layout_runtime();
        let selected = signal(0u32);
        let mut item = tabs(TabsProps {
            items: vec!["One", "Two"],
            selected: Some(selected.clone()),
            ..Default::default()
        })
        .unwrap();
        let r = lay_out(item.layout_node());
        let (cx, cy) = ((r.x + r.width - 2.0) as f64, (r.y + r.height / 2.0) as f64);

        item.on_event(&press(cx, cy));
        item.on_event(&release(cx, cy));

        assert_eq!(
            selected.get(),
            1,
            "pressing the last (second) tab sets selected to its index"
        );
    }
}
