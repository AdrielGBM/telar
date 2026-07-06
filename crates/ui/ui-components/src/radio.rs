use std::rc::Rc;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke, TextStyle};
use theme_core::use_widget_theme;
use ui_core::{Container, LayoutItem, StyledContainer, Text, WidgetCtx, box_item};

/// Fallback accent when no reactive `color` is supplied and no theme is active (matches `Button`'s default primary).
const DEFAULT_ACCENT: Color = Color::rgba(0.24, 0.47, 0.98, 1.0);
/// The ring's fill, its unselected border, and the label ink.
const SURFACE: Color = Color::WHITE;
const BORDER: Color = Color::rgba(0.75, 0.77, 0.80, 1.0);
const LABEL: Color = Color::rgba(0.15, 0.15, 0.2, 1.0);

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

pub fn radio(ctx: &mut WidgetCtx, props: RadioProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
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
    let color: Rc<dyn Fn() -> Color> = Rc::from(color);

    // The dot: an inner circle that paints the accent only while this value is selected, so selecting never reflows.
    let dot_selected = selected.clone();
    let dot_color = color.clone();
    let dot = StyledContainer::new(
        ctx,
        LayoutStyle::new().width(10.0).height(10.0),
        move |_r| {
            let fill = if dot_selected.get() == value {
                accent(dot_color.as_ref())
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
        ctx,
        LayoutStyle::new()
            .flex_row()
            .width(18.0)
            .height(18.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        move |_r| {
            let stroke = if ring_selected.get() == value {
                accent(ring_color.as_ref())
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

    let mut children: Vec<Box<dyn LayoutItem>> = vec![box_item(ring)];
    if !label.is_empty() {
        // `auto` (a measured leaf) so the label gets its intrinsic WIDTH in this row — a plain `Text::new`
        // only stretches its cross-axis (height here), leaving width 0 and the label invisible.
        let text = Text::auto(
            ctx,
            move || label.to_string(),
            LayoutStyle::new(),
            || TextStyle::new(14.0, LABEL),
        )?;
        children.push(box_item(text));
    }

    // The whole row is the tap target (ring + label); a tap selects this value and reports it.
    let select = selected.clone();
    let row = Container::new(
        ctx,
        LayoutStyle::new()
            .flex_row()
            .gap(10.0)
            .align_items(AlignItems::CENTER),
        children,
    )?
    .on_press(move || {
        select.set(value);
        if let Some(cb) = &on_select {
            cb(value);
        }
    });
    Ok(box_item(row))
}

/// The accent colour: the caller's reactive `color` if set, else the theme's widget primary (as `Button` does).
fn accent(color: &dyn Fn() -> Color) -> Color {
    let c = color();
    if c == Color::TRANSPARENT {
        use_widget_theme()
            .map(|t| t.widget_primary())
            .unwrap_or(DEFAULT_ACCENT)
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use layout_core::{AvailableSpace, LayoutStyle};
    use platform_core::{Event, PointerButton, PointerSource};
    use reactive_core::signal;
    use ui_core::{
        Component, LayoutItem, NodeId, WidgetCtx, compute_layout, new_container, track_layout,
    };

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
    fn lay_out(ctx: &mut WidgetCtx, node: NodeId) -> (f64, f64) {
        let rect = track_layout(ctx, node).unwrap();
        let root = new_container(
            ctx,
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            &[node],
        )
        .unwrap();
        compute_layout(
            ctx,
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
        let mut ctx = WidgetCtx::new();
        let selected = signal(0u32);
        let mut widget = radio(
            &mut ctx,
            RadioProps {
                selected: Some(selected.clone()),
                value: 2,
                label: "Large",
                ..Default::default()
            },
        )
        .unwrap();
        let (cx, cy) = lay_out(&mut ctx, widget.layout_node());

        assert_eq!(selected.get(), 0, "starts unselected (group is on 0)");
        widget.on_event(&press(cx, cy));
        widget.on_event(&release(cx, cy));
        assert_eq!(selected.get(), 2, "a tap selects this button's value");
    }

    #[test]
    fn on_select_reports_value() {
        let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let mut ctx = WidgetCtx::new();
        let mut widget = radio(
            &mut ctx,
            RadioProps {
                selected: Some(signal(0u32)),
                value: 5,
                on_select: Some(Box::new(move |v| sink.set(Some(v)))),
                ..Default::default()
            },
        )
        .unwrap();
        let (cx, cy) = lay_out(&mut ctx, widget.layout_node());

        widget.on_event(&press(cx, cy));
        widget.on_event(&release(cx, cy));
        assert_eq!(
            seen.get(),
            Some(5),
            "on_select fires with this button's value"
        );
    }
}
