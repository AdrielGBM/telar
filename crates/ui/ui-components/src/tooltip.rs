use std::rc::Rc;

use layout_core::{AlignItems, LayoutError, LayoutStyle};
use reactive_core::signal;
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, TextStyle};
use ui_core::{
    Container, LayoutItem, Overlay, ReactiveList, Slots, StyledContainer, Text, WidgetCtx,
    box_item, track_layout,
};

/// Fallback bubble surface when `color` is unset — an opaque dark chip.
const DEFAULT_BUBBLE: Color = Color::rgba(0.12, 0.12, 0.16, 0.96);
/// Bubble text colour (always light, on the dark chip).
const BUBBLE_INK: Color = Color::rgba(0.98, 0.98, 1.0, 1.0);
const BUBBLE_RADIUS: f32 = 6.0;
const BUBBLE_PAD_X: f32 = 8.0;
const BUBBLE_PAD_Y: f32 = 5.0;
const BUBBLE_TEXT_SIZE: f32 = 12.0;

/// A hover popup: wraps its slot (the trigger content) and, while the mouse is over it, shows a small `text`
/// bubble anchored just below the trigger. Built on the `overlay` primitive's anchored variant (the bubble is
/// portalled to the top layer and translated to the trigger's rect, so it escapes clipping and only itself
/// blocks). High-level sugar; lives in `ui-components`, not the kernel.
pub struct TooltipProps {
    pub text: &'static str,
    /// Bubble surface colour. `Color::TRANSPARENT` (the default) means "unset" -> `DEFAULT_BUBBLE`. A closure
    /// (re-read every frame) so a theme token or `$signal` colour re-colours live.
    pub color: Box<dyn Fn() -> Color>,
}

impl Default for TooltipProps {
    fn default() -> Self {
        Self {
            text: "",
            color: Box::new(|| Color::TRANSPARENT),
        }
    }
}

pub fn tooltip(
    ctx: &mut WidgetCtx,
    props: TooltipProps,
    mut slots: Slots,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let TooltipProps { text, color } = props;
    let trigger_content = slots.take_default();
    // Reflects whether the mouse is over the trigger; drives the bubble's show/hide.
    let hovered = signal(false);

    let hover_sink = hovered.clone();
    let trigger = StyledContainer::new(
        ctx,
        LayoutStyle::new().flex_row(),
        |_r| RectStyle::default(),
        trigger_content,
    )?
    .on_hover(move |over| hover_sink.set(over));
    let trigger_node = trigger.layout_node();
    // The trigger's laid-out rect (a fresh runtime handle, not the borrowed `ctx`): the anchored overlay
    // reads it to position the bubble below the trigger.
    let trigger_rect =
        track_layout(&WidgetCtx::handle(), trigger_node).expect("trigger container is registered");

    // The bubble is a fresh `text` each hover (rebuildable — no slot children to preserve), so no take-once
    // cell here; keying on `hovered` mounts/disposes the anchored overlay like a reactive `if`.
    let color: Rc<dyn Fn() -> Color> = Rc::from(color);
    let key_hovered = hovered.clone();
    let bubble = ReactiveList::new(
        ctx,
        move || vec![key_hovered.get()],
        |is_hovered: &bool| *is_hovered,
        move |ctx, is_hovered| -> Result<Box<dyn LayoutItem>, LayoutError> {
            if !is_hovered {
                return Ok(box_item(Container::new(
                    ctx,
                    LayoutStyle::new().width(0.0).height(0.0),
                    vec![],
                )?));
            }
            build_bubble(ctx, text, color.clone(), trigger_node, trigger_rect.clone())
        },
    )?;

    // The trigger sits in flow; the bubble node is a 0-size portal placeholder, so it never shifts the trigger.
    Ok(box_item(Container::new(
        ctx,
        LayoutStyle::new().flex_column(),
        vec![box_item(trigger), box_item(bubble)],
    )?))
}

/// Builds the bubble for the hovered state: a padded rounded chip with the tooltip `text`, positioned just
/// below the trigger (via `absolute_rect`, so it works even when the trigger is in a separately-computed
/// sub-root) inside a NON-blocking overlay (a tooltip must not eat clicks on the page).
fn build_bubble(
    ctx: &mut WidgetCtx,
    text: &'static str,
    color: Rc<dyn Fn() -> Color>,
    trigger_node: ui_core::NodeId,
    trigger_rect: reactive_core::RwSignal<geometry_core::Rect>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // Window-absolute trigger position (the overlay hoists to the window); fall back to the local rect.
    let anchor = ui_core::absolute_rect(trigger_node).unwrap_or_else(|| trigger_rect.peek());
    // `auto` (measured) so the bubble text gets its intrinsic WIDTH in the row chip; a plain `Text::new`
    // only stretches its cross-axis (height in a row), leaving width 0 and the tooltip empty.
    let label = Text::auto(
        ctx,
        move || text.to_string(),
        LayoutStyle::new(),
        || TextStyle::new(BUBBLE_TEXT_SIZE, BUBBLE_INK),
    )?;
    let chip = StyledContainer::new(
        ctx,
        LayoutStyle::new()
            .flex_row()
            .padding_horizontal(BUBBLE_PAD_X)
            .padding_vertical(BUBBLE_PAD_Y)
            .margin_left(anchor.x)
            .margin_top(anchor.y + anchor.height),
        move |_r| {
            RectStyle::default()
                .with_fill(bubble_fill(color.as_ref()))
                .with_radius(BorderRadius::all(BUBBLE_RADIUS))
        },
        vec![box_item(label)],
    )?;
    // `align_items(START)` so the chip sizes to its content instead of stretching to the full-viewport fill.
    let overlay = Overlay::new_click_through(
        ctx,
        LayoutStyle::new().align_items(AlignItems::START),
        vec![box_item(chip)],
    )?;
    Ok(box_item(overlay))
}

/// The bubble surface: the caller's reactive `color` if set, else the opaque dark default chip.
fn bubble_fill(color: &dyn Fn() -> Color) -> Color {
    let c = color();
    if c == Color::TRANSPARENT {
        DEFAULT_BUBBLE
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerSource};
    use renderer_core::DrawCommand;
    use ui_core::{ComponentList, compute_layout, new_container, relayout_if_dirty};

    fn find_text(cmds: &[DrawCommand], needle: &str) -> bool {
        cmds.iter()
            .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.as_ref() == needle))
    }

    fn moved(x: f64, y: f64) -> Event {
        Event::PointerMoved {
            x,
            y,
            source: PointerSource::Mouse,
        }
    }

    // A trigger slot child with a definite size so the trigger has a rect to hover and anchor against.
    fn slot_with_trigger(ctx: &mut WidgetCtx) -> Slots {
        let inner =
            Container::new(ctx, LayoutStyle::new().width(80.0).height(30.0), vec![]).unwrap();
        let mut slots = Slots::new();
        slots.push(None, box_item(inner));
        slots
    }

    // Hovering the trigger shows the bubble; leaving hides it. Driven through the full component tree, like
    // a real app: a mouse move onto the trigger fires its on_hover, mounting the anchored overlay.
    #[test]
    fn hover_shows_bubble_and_leave_hides_it() {
        let mut ctx = WidgetCtx::new();
        let slots = slot_with_trigger(&mut ctx);
        let tooltip = tooltip(
            &mut ctx,
            TooltipProps {
                text: "Helpful hint",
                ..Default::default()
            },
            slots,
        )
        .unwrap();

        // A parent-less root computed against the window registers the overlay host the bubble anchors into.
        let root = new_container(
            &mut ctx,
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[tooltip.layout_node()],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(tooltip);
        let _ = tree.commands();

        assert!(!find_text(&tree.commands(), "Helpful hint"));

        // Move onto the trigger (its rect is ~0,0,80,30): on_hover(true) mounts the bubble.
        tree.on_event(&moved(40.0, 15.0));
        relayout_if_dirty();
        assert!(
            find_text(&tree.commands(), "Helpful hint"),
            "bubble shows on hover"
        );

        // Move far away: on_hover(false) disposes the bubble.
        tree.on_event(&moved(9999.0, 9999.0));
        relayout_if_dirty();
        assert!(
            !find_text(&tree.commands(), "Helpful hint"),
            "bubble hidden on leave"
        );
    }

    // Construction succeeds headless with an empty trigger and no hover.
    #[test]
    fn builds_without_hover() {
        let mut ctx = WidgetCtx::new();
        let slots = slot_with_trigger(&mut ctx);
        let tooltip = tooltip(&mut ctx, TooltipProps::default(), slots).unwrap();
        let tree = ComponentList::new(tooltip);
        assert!(!find_text(&tree.commands(), "Helpful hint"));
    }
}
