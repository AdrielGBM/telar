use std::rc::Rc;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke};
use theme_core::use_theme_tokens;
use ui_core::{LayoutItem, StyledContainer, box_item};

use crate::shared;

/// The ring's fill and its unselected border.
const SURFACE: Color = Color::WHITE;
const BORDER: Color = Color::rgba(0.75, 0.77, 0.80, 1.0);

/// One radio button in a group: an 18px ring that fills its centre dot (and accents its border) while the
/// bound `selected` signal equals this button's `value`; tapping the row sets `selected` to `value` (and fires
/// `on_select`). A radio *group* is several `radio`s sharing one `selected` signal with different `value`s.
/// High-level sugar over the primitives (`box` + `on_press` + a reactive fill); lives in `ui-components`, not
/// the kernel. `selected` is `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget owns
/// its own signal, so it never matches — a lone default radio), `Some` is the shared group signal.
pub struct RadioProps {
    /// The group's bound selection. `None` (the default) is uncontrolled — the widget makes its own `signal(0)`.
    pub selected: Option<RwSignal<u32>>,
    /// This button's value: it is selected when `selected` equals it, and a tap sets `selected` to it.
    pub value: u32,
    pub label: &'static str,
    /// Accent (the selected dot and border). `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    pub color: Box<dyn Fn() -> Color>,
    /// Fires with this button's `value` when it becomes selected.
    pub on_select: Option<Box<dyn Fn(u32)>>,
}

impl Default for RadioProps {
    fn default() -> Self {
        Self {
            selected: None,
            value: 0,
            label: "",
            color: Box::new(|| Color::TRANSPARENT),
            on_select: None,
        }
    }
}

pub fn radio(props: RadioProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let RadioProps {
        selected,
        value,
        label,
        color,
        on_select,
    } = props;
    // Uncontrolled: own the selection so the widget is self-consistent when the caller binds no group signal.
    let selected = selected.unwrap_or_else(|| signal(0u32));
    // Shared across the dot and ring style closures (a `Box<dyn Fn>` is not `Clone`, an `Rc` handle is).
    let color: shared::ReactiveColor = Rc::from(color);

    // The dot: an inner circle that paints the accent only while this value is selected, so selecting never reflows.
    let dot_selected = selected.clone();
    let dot_color = color.clone();
    let dot = StyledContainer::new(
        LayoutStyle::new().width(10.0).height(10.0),
        move |_r| {
            let fill = if dot_selected.get() == value {
                shared::resolve(dot_color.as_ref(), || {
                    use_theme_tokens()
                        .map(|t| t.primary())
                        .unwrap_or(shared::DEFAULT_ACCENT)
                })
            } else {
                Color::TRANSPARENT
            };
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(5.0))
        },
        vec![],
    )?;

    // The 18px ring: white with an accent border when selected, a neutral border otherwise, dot centred inside.
    let ring_selected = selected.clone();
    let ring_color = color.clone();
    let ring = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .width(18.0)
            .height(18.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        move |_r| {
            let stroke = if ring_selected.get() == value {
                shared::resolve(ring_color.as_ref(), || {
                    use_theme_tokens()
                        .map(|t| t.primary())
                        .unwrap_or(shared::DEFAULT_ACCENT)
                })
            } else {
                BORDER
            };
            RectStyle::default()
                .with_fill(SURFACE)
                .with_stroke(Stroke::new(stroke, 1.5))
                .with_radius(BorderRadius::all(9.0))
        },
        vec![box_item(dot)],
    )?;

    // The whole row is the tap target (ring + label); a tap selects this value and reports it.
    let select = selected.clone();
    shared::labelled_control(box_item(ring), label, move || {
        select.set(value);
        if let Some(cb) = &on_select {
            cb(value);
        }
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use ui_core::reset_layout_runtime;

    use layout_core::{AvailableSpace, LayoutStyle};
    use platform_core::{Event, PointerButton, PointerSource};
    use reactive_core::signal;
    use ui_core::{Component, LayoutItem, NodeId, compute_layout, new_container, track_layout};

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

    // Lays `node` out inside a 200×100 root and returns its centre point, for tapping.
    fn lay_out(node: NodeId) -> (f64, f64) {
        let rect = track_layout(node).unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            &[node],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        let r = rect.get();
        ((r.x + r.width / 2.0) as f64, (r.y + r.height / 2.0) as f64)
    }

    #[test]
    fn tap_sets_group_selection_to_value() {
        reset_layout_runtime();
        let selected = signal(0u32);
        let mut widget = radio(RadioProps {
            selected: Some(selected.clone()),
            value: 2,
            label: "Large",
            ..Default::default()
        })
        .unwrap();
        let (cx, cy) = lay_out(widget.layout_node());

        assert_eq!(selected.get(), 0, "starts unselected (group is on 0)");
        widget.on_event(&press(cx, cy));
        widget.on_event(&release(cx, cy));
        assert_eq!(selected.get(), 2, "a tap selects this button's value");
    }

    #[test]
    fn on_select_reports_value() {
        let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        reset_layout_runtime();
        let mut widget = radio(RadioProps {
            selected: Some(signal(0u32)),
            value: 5,
            on_select: Some(Box::new(move |v| sink.set(Some(v)))),
            ..Default::default()
        })
        .unwrap();
        let (cx, cy) = lay_out(widget.layout_node());

        widget.on_event(&press(cx, cy));
        widget.on_event(&release(cx, cy));
        assert_eq!(
            seen.get(),
            Some(5),
            "on_select fires with this button's value"
        );
    }
}
