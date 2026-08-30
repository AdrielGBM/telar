use telar_macros::Props;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::{Reactive, RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, TextStyle};
use ui_core::focus::Role;
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

/// A horizontal tab bar: one button per label, driving a `selected` index. Renders only the row of tab
/// buttons — the matching content panel is the caller's responsibility (typically the DSL's reactive
/// `if selected == i`, mirroring the sandbox's own nav-button/section-switch split). Modelled on
/// `select.rs`'s items/selected handling, but rendered inline (a row) rather than as an anchored overlay.
/// High-level sugar over the primitives; lives in `ui-components`, not the kernel.
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

pub fn tabs(props: TabsProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let TabsProps {
        items,
        selected,
        color,
    } = props;
    // Uncontrolled: own the index so the bar still tracks the active tab when the caller binds no signal.
    let selected = selected.unwrap_or_else(|| signal(0u32));
    // Erased to `Rc` so the same colour closure feeds every tab's fill, hover and label style.

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

/// The tab pill's paint: the active tab fills with the accent (a touch darker on hover); an inactive tab
/// blends in until hovered, when it lifts to a faint accent wash.
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

/// A tab's label: legible on the accent pill when it is the selected one, and otherwise a quieter shade of
/// whatever the bar around it is written in.
///
/// The inactive tone was a flat grey literal — the same grey in dark mode, and the same grey under a region
/// that had declared its own ink. It was also, to two decimals, what fading the default ink over a white page
/// produces: a light-mode screenshot of a value the cascade already knows how to work out.
fn tab_text(inherited: TextStyle, active: bool) -> TextStyle {
    if active {
        shared::control_text(inherited, 1.0).with_color(shared::on_accent())
    } else {
        shared::quiet(inherited, 1.0)
    }
}

#[cfg(test)]
mod tests {

    use layout_core::AvailableSpace;
    use renderer_core::DrawCommand;
    use ui_core::{Component, ComponentList, NodeId, compute_layout, new_container};

    use super::*;
    use crate::harness::{press, release};

    /// A bar in a region that declared its own ink writes the tabs that are not selected in a quieter shade
    /// of *that*, rather than in a grey no theme and no declaration can move.
    #[test]
    fn an_inactive_tab_fades_the_ink_of_the_region_around_it() {
        crate::test_support::fresh_layout_runtime();
        let item = tabs(TabsProps::props().items(vec!["One", "Two"]).build()).unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(100.0),
            &[item.layout_node()],
        )
        .unwrap();
        let declared = Color::rgba(0.9, 0.2, 0.1, 1.0);
        ui_core::declare(
            root,
            renderer_core::Declared::default().with_color(declared),
        );
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let tree = ComponentList::new(item);
        let ink = tree
            .commands()
            .iter()
            .find_map(|c| match c {
                DrawCommand::Text { text, style, .. } if text.as_ref() == "Two" => {
                    Some(style.color.solid_color())
                }
                _ => None,
            })
            .expect("the bar drew the tab that is not selected");
        assert_eq!(ink, declared.with_alpha(shared::QUIET_ALPHA));
    }

    // Lays `node` out as the sole child of a 400×100 root (an auto-size ROOT fills its available space, so
    // laying `node` itself out as the root would force it to 400px wide instead of its natural content
    // width) and returns its laid-out rect.
    fn lay_out(node: NodeId) -> geometry_core::Rect {
        crate::harness::lay_out_row(node, 400.0, 100.0)
    }

    // Construction: a tab bar builds headless, lays out (a row of measured tab pills), and renders.
    #[test]
    fn builds_and_lays_out() {
        crate::test_support::fresh_layout_runtime();
        let item = tabs(
            TabsProps::props()
                .items(vec!["One", "Two", "Three"])
                .build(),
        )
        .unwrap();
        let r = lay_out(item.layout_node());
        assert!(r.width > 0.0, "the tab row takes some width");
        let _ = item.view();
    }

    // With exactly two content-sized tabs and no `flex_grow` on the row, its right edge sits flush against the second tab's own padding, so a point just inside that edge lands on index 1 without hardcoding font metrics.
    #[test]
    fn pressing_the_last_tab_sets_selected_index() {
        crate::test_support::fresh_layout_runtime();
        let selected = signal(0u32);
        let mut item = tabs(
            TabsProps::props()
                .items(vec!["One", "Two"])
                .selected(selected)
                .build(),
        )
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
