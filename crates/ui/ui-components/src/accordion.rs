use std::rc::Rc;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::{Effect, RwSignal, effect, signal};
use renderer_core::{Color, RectStyle, TextStyle};
use theme_core::use_widget_theme;
use ui_core::{
    ClippedItem, Component, Container, EventResult, LayoutItem, RenderNode, Slots, StyledContainer,
    Text, box_item, mark_dirty, set_display,
};

use crate::shared;

const PAD_X: f32 = 14.0;
const PAD_Y: f32 = 10.0;
const TITLE_SIZE: f32 = 14.0;
const CARET_SIZE: f32 = 12.0;
const CARET_CLOSED: &str = "\u{25B8}"; // ▸
const CARET_OPEN: &str = "\u{25BE}"; // ▾

/// A single collapsible section: a clickable header (title + caret) that toggles an `open` bool, and the
/// slot children below it, present only while open. This is high-level sugar over the primitives; lives in
/// `ui-components`, not the kernel, so an app can drop it or ship its own.
///
/// The body is built ONCE from the slot children (they arrive as pre-built widgets and cannot be rebuilt
/// once consumed, like `modal`/`drawer`'s), then shown/hidden by toggling `display:none` on its layout node
/// — the same mechanism the sandbox uses to switch doc sections — instead of a `ReactiveList` rebuild.
/// Unlike `modal`/`drawer`'s `Overlay`, this stays IN FLOW: collapsing the body's node to a zero rect pushes
/// following siblings back up, and expanding it pushes them down again.
pub struct AccordionProps {
    /// Header label.
    pub title: &'static str,
    /// Bound open/closed state. `None` (the default) is uncontrolled — the widget owns its own `signal(false)`.
    pub open: Option<RwSignal<bool>>,
    /// Accent (the caret). `Color::TRANSPARENT` (the default) means "unset": falls back to the theme accent.
    pub color: Box<dyn Fn() -> Color>,
}

impl Default for AccordionProps {
    fn default() -> Self {
        Self {
            title: "",
            open: None,
            color: Box::new(|| Color::TRANSPARENT),
        }
    }
}

pub fn accordion(
    props: AccordionProps,
    mut slots: Slots,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let AccordionProps { title, open, color } = props;
    // Uncontrolled: own the state so the section still expands/collapses when the caller binds no signal.
    let open = open.unwrap_or_else(|| signal(false));
    let color: shared::ReactiveColor = Rc::from(color);
    let body_children = slots.take_default();

    // Caret flips glyph with `open`; its colour is the same reactive accent as the rest of the catalogue.
    let caret_open = open.clone();
    let caret_color = color.clone();
    let caret = Text::auto(
        move || {
            if caret_open.get() {
                CARET_OPEN
            } else {
                CARET_CLOSED
            }
            .to_string()
        },
        LayoutStyle::new(),
        move || {
            let accent = shared::resolve(caret_color.as_ref(), || {
                use_widget_theme()
                    .map(|t| t.widget_primary())
                    .unwrap_or(shared::DEFAULT_ACCENT)
            });
            TextStyle::new(CARET_SIZE, accent)
        },
    )?;

    let title_widget = Text::auto(
        move || title.to_string(),
        LayoutStyle::new(),
        || TextStyle::new(TITLE_SIZE, shared::ink()),
    )?;

    let toggle_open = open.clone();
    let header = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN)
            .gap(8.0)
            .padding_horizontal(PAD_X)
            .padding_vertical(PAD_Y),
        |_r| RectStyle::default(),
        vec![box_item(title_widget), box_item(caret)],
    )?
    .on_press(move || toggle_open.update(|o| *o = !*o));

    // Body: built once from the slot children. Clipped to its own rect so a collapsed (zero-rect) body
    // draws nothing even if a child paints at fixed coordinates (mirrors `ClippedItem`'s own doc example).
    let body = Container::new(LayoutStyle::new().flex_column(), body_children)?;
    let body_node = body.layout_node();
    let clipped_body = ClippedItem::new(box_item(body));

    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(header), box_item(clipped_body)],
    )?;

    // Drives the body's `display` from `open`: collapses it out of flow when closed, restores it when open.
    // Runs once immediately (setting the initial display) and again on every toggle; kept alive by `Accordion`.
    let display_open = open.clone();
    let _effect = effect(move || {
        set_display(body_node, display_open.get());
        let _ = mark_dirty(body_node);
    });

    Ok(box_item(Accordion { root, _effect }))
}

/// Wraps the header+body column together with the display-toggling effect so it stays alive for the
/// widget's lifetime — an `Effect` deregisters itself on drop, same as `ReactiveList`'s own `_effect` field.
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
    use ui_core::reset_layout_runtime;

    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerButton, PointerSource};
    use ui_core::{compute_layout, new_container, track_layout};

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

    fn slot_with_body(label: &'static str) -> Slots {
        let body = Text::new(
            move || label.to_string(),
            LayoutStyle::new().height(20.0),
            || TextStyle::new(14.0, Color::BLACK),
        )
        .unwrap();
        let mut slots = Slots::new();
        slots.push(None, box_item(body));
        slots
    }

    // Pressing the header toggles the bound `open` signal.
    #[test]
    fn pressing_the_header_toggles_open() {
        reset_layout_runtime();
        let open = signal(false);
        let mut item = accordion(
            AccordionProps {
                title: "Details",
                open: Some(open.clone()),
                ..Default::default()
            },
            slot_with_body("Body"),
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
        // The header is the first (top) row, so a point near the top-left always lands on it regardless of
        // the closed body's collapsed height.
        let (cx, cy) = ((r.x + 10.0) as f64, (r.y + 5.0) as f64);

        item.on_event(&press(cx, cy));
        item.on_event(&release(cx, cy));
        assert!(open.get(), "a tap on the header opens the section");

        item.on_event(&press(cx, cy));
        item.on_event(&release(cx, cy));
        assert!(!open.get(), "a second tap closes it again");
    }

    // Closed, the body's node collapses to a zero rect (out of flow); opening it restores its natural size.
    // Laid out as a CHILD of a fixed-size wrapper (not as the layout root itself): an auto-height root fills
    // its given available space, which would stretch the accordion to 400px both closed and open and hide
    // the very difference this test checks (mirrors `checkbox.rs`'s `lay_out` / `drawer.rs`'s test wrapper).
    #[test]
    fn open_expands_body_and_close_collapses_it() {
        reset_layout_runtime();
        let open = signal(false);
        let item = accordion(
            AccordionProps {
                title: "Details",
                open: Some(open.clone()),
                ..Default::default()
            },
            slot_with_body("Body"),
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

    // An unbound accordion (no `open` signal) still builds and starts collapsed: only the header
    // contributes height, well under what the header plus the 20px body row would add up to.
    #[test]
    fn uncontrolled_accordion_builds_and_starts_closed() {
        reset_layout_runtime();
        let item = accordion(
            AccordionProps {
                title: "Details",
                ..Default::default()
            },
            slot_with_body("Body"),
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
