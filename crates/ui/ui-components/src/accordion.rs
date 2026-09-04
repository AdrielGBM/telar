//! [`accordion`]: a header that expands and collapses the body beneath it.

use telar_macros::Props;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::{Effect, Reactive, RwSignal, effect, signal};
use renderer_core::{Color, RectStyle};
#[cfg(test)]
use ui_core::Slots;
use ui_core::focus::Role;
use ui_core::{
    Children, Clip, ClippedItem, Component, Container, EventResult, LayoutItem, RenderNode,
    StyledContainer, Text, box_item, mark_dirty, set_display,
};

use crate::shared;

fn pad_x() -> f32 {
    shared::spacing() * 1.75
}
fn pad_y() -> f32 {
    shared::spacing() * 1.25
}
/// The caret's share of the text beside it.
const CARET_RATIO: f32 = 0.85;
const CARET_CLOSED: &str = "\u{25B8}"; // ▸
const CARET_OPEN: &str = "\u{25BE}"; // ▾

fn header_box() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::SPACE_BETWEEN)
        .gap(8.0)
        .padding_horizontal(pad_x())
        .padding_vertical(pad_y())
}

/// A single collapsible section: a clickable header (title + caret) that toggles an `open` bool, and the slot children below it, present only while open. This is high-level sugar over the primitives; lives in `ui-components`, not the kernel, so an app can drop it or ship its own.
///
/// The body is built ONCE from the slot children (they arrive as pre-built widgets and cannot be rebuilt once consumed, like `modal`/`drawer`'s), then shown/hidden by toggling `display:none` on its layout node — the same mechanism the sandbox uses to switch doc sections — instead of a `ReactiveList` rebuild. Unlike `modal`/`drawer`'s `Overlay`, this stays IN FLOW: collapsing the body's node to a zero rect pushes following siblings back up, and expanding it pushes them down again.
#[derive(Props)]
pub struct AccordionProps {
    /// Header label.
    #[props(into, default)]
    pub title: Reactive<String>,
    /// Bound open/closed state. `None` (the default) is uncontrolled — the widget owns its own `signal(false)`.
    #[props(some, into, default)]
    pub open: Option<RwSignal<bool>>,
    /// Accent (the caret). `Color::TRANSPARENT` (the default) means "unset": falls back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
}

/// A header that expands and collapses the body nested inside it.
pub fn accordion(
    props: AccordionProps,
    children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut slots = children.build()?;
    let AccordionProps { title, open, color } = props;
    // Uncontrolled: own the state so the section still expands/collapses when the caller binds no signal.
    let open = open.unwrap_or_else(|| signal(false));
    let body_children = slots.take_default();

    let caret_open = open;
    let caret_color = color.clone();
    let caret = Text::declaring(
        move || {
            if caret_open.get() {
                CARET_OPEN
            } else {
                CARET_CLOSED
            }
            .to_string()
        },
        LayoutStyle::new(),
        move |t| {
            let accent = shared::resolve(&caret_color, shared::accent);
            shared::control_text(t, CARET_RATIO).with_color(accent)
        },
    )?;

    let title_widget = Text::declaring(
        move || title.get(),
        LayoutStyle::new(),
        |t| shared::control_text(t, 1.0),
    )?;

    let announced_open = open;
    let toggle_open = open;
    let header = StyledContainer::new(
        header_box(),
        |_r| RectStyle::default(),
        vec![box_item(title_widget), box_item(caret)],
    )?
    .styled_by(header_box)
    .control(Role::Disclosure)
    .toggled(move || announced_open.get())
    .on_press(move || toggle_open.update(|o| *o = !*o));

    // Built once from the slot children, and clipped to its own rect so a collapsed body draws nothing even if a child paints at fixed coordinates.
    let body = Container::new(LayoutStyle::new().flex_column(), body_children)?;
    let body_node = body.layout_node();
    let clipped_body = ClippedItem::new(box_item(body), Clip::both());

    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(header), box_item(clipped_body)],
    )?;

    // Collapses the body out of flow when closed and restores it when open. Runs once immediately for the initial display; kept alive by `Accordion`.
    let display_open = open;
    let _effect = effect(move || {
        set_display(body_node, display_open.get());
        let _ = mark_dirty(body_node);
    });

    Ok(box_item(Accordion { root, _effect }))
}

/// Wraps the header+body column together with the display-toggling effect so it stays alive for the widget's lifetime — an `Effect` deregisters itself on drop, same as `ReactiveList`'s own `_effect` field.
struct Accordion {
    root: Container,
    _effect: Effect,
}

impl LayoutItem for Accordion {
    fn layout_node(&self) -> NodeId {
        self.root.layout_node()
    }
}

impl Component for Accordion {
    fn view(&self) -> RenderNode {
        self.root.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.root.on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        "Accordion"
    }
}

#[cfg(test)]
mod tests {
    use layout_core::AvailableSpace;

    use ui_core::{compute_layout, new_container, track_layout};

    use super::*;
    use crate::harness::{press, release};

    fn slot_with_body(label: &'static str) -> Slots {
        let body = Text::declaring(
            move || label.to_string(),
            LayoutStyle::new().height(20.0),
            |t| t,
        )
        .unwrap();
        let mut slots = Slots::new();
        slots.push(None, box_item(body));
        slots
    }

    #[test]
    fn pressing_the_header_toggles_open() {
        crate::test_support::fresh_layout_runtime();
        let open = signal(false);
        let mut item = accordion(
            AccordionProps::props().title("Details").open(open).build(),
            Children::from(slot_with_body("Body")),
        )
        .unwrap();
        let node = item.layout_node();
        let rect = track_layout(node).unwrap();
        compute_layout(
            node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let r = rect.get();
        // The header is the top row, so a point near the top-left lands on it whatever the collapsed body's height.
        let (cx, cy) = ((r.x + 10.0) as f64, (r.y + 5.0) as f64);

        item.on_event(&press(cx, cy));
        item.on_event(&release(cx, cy));
        assert!(open.get(), "a tap on the header opens the section");

        item.on_event(&press(cx, cy));
        item.on_event(&release(cx, cy));
        assert!(!open.get(), "a second tap closes it again");
    }

    // Laid out as a child of a fixed-size wrapper, not as the layout root: an auto-height root fills its available space, which would stretch the accordion to 400px both closed and open and hide the difference.
    #[test]
    fn open_expands_body_and_close_collapses_it() {
        crate::test_support::fresh_layout_runtime();
        let open = signal(false);
        let item = accordion(
            AccordionProps::props().title("Details").open(open).build(),
            Children::from(slot_with_body("Body")),
        )
        .unwrap();
        let node = item.layout_node();
        let root_rect = track_layout(node).unwrap();
        let wrapper = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[node],
        )
        .unwrap();
        compute_layout(
            wrapper,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let closed_height = root_rect.get().height;

        open.set(true);
        compute_layout(
            wrapper,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let open_height = root_rect.get().height;

        assert!(
            open_height > closed_height,
            "opening the section grows the root's height (closed: {closed_height}, open: {open_height})"
        );
    }

    #[test]
    fn uncontrolled_accordion_builds_and_starts_closed() {
        crate::test_support::fresh_layout_runtime();
        let item = accordion(
            AccordionProps::props().title("Details").build(),
            Children::from(slot_with_body("Body")),
        )
        .unwrap();
        let node = item.layout_node();
        let root_rect = track_layout(node).unwrap();
        let wrapper = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[node],
        )
        .unwrap();
        compute_layout(
            wrapper,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let height = root_rect.get().height;
        assert!(
            height > 0.0 && height <= 55.0,
            "closed, only the header should contribute height, got {height}"
        );
    }
}
