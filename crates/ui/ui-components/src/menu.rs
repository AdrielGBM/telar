use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{AlignItems, LayoutError, LayoutStyle};
use reactive_core::signal;
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke, TextStyle};
use theme_core::use_widget_theme;
use ui_core::{
    Container, LayoutItem, Overlay, ReactiveList, StyledContainer, Text, WidgetCtx, box_item,
    track_layout,
};

/// The trigger's fixed size and the dropdown panel's geometry. Kept as constants (not content-sized) so the
/// panel lines up under the trigger and so the widget's hit geometry is deterministic (its tests compute
/// click points from these).
const PANEL_WIDTH: f32 = 180.0;
const TRIGGER_HEIGHT: f32 = 36.0;
const ROW_HEIGHT: f32 = 32.0;
const PANEL_PAD: f32 = 4.0;

const INK: Color = Color::rgba(0.12, 0.13, 0.16, 1.0);
const SURFACE: Color = Color::rgba(1.0, 1.0, 1.0, 1.0);
const BORDER: Color = Color::rgba(0.85, 0.86, 0.9, 1.0);

/// A click-triggered list of action items: a labelled trigger button that opens an anchored list; picking an
/// item fires `on_select` with its index and closes. Unlike `select`, a menu holds no bound selection state —
/// its items are one-shot actions. High-level sugar built on the overlay anchor + click-through primitives;
/// lives in `ui-components`, not the kernel.
pub struct MenuProps {
    /// The trigger button's label.
    pub label: &'static str,
    /// The action items, listed in the panel in order.
    pub items: Vec<&'static str>,
    /// Fired with the index of the chosen item when it is picked.
    pub on_select: Option<Box<dyn Fn(u32)>>,
    /// Accent colour (trigger border, hover highlight). `Color::TRANSPARENT` (the default) means "unset" and
    /// falls back to the theme accent. A closure so a theme token re-reads on every render.
    pub color: Box<dyn Fn() -> Color>,
}

impl Default for MenuProps {
    fn default() -> Self {
        Self {
            label: "",
            items: Vec::new(),
            on_select: None,
            color: Box::new(|| Color::TRANSPARENT),
        }
    }
}

// NOTE on the overlay: the panel is positioned against the trigger by layout margins (world coordinates)
// inside a BLOCKING overlay with a transparent full-viewport backdrop — the backdrop dismisses the menu on a
// click-away and stops hover/clicks bleeding through to the page behind. The trigger's rect is read once at
// open time, so the panel does not re-follow a trigger that moves while open (a menu is short-lived).
pub fn menu(ctx: &mut WidgetCtx, props: MenuProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = props.label;
    let items = props.items;
    // Erased to `Rc` so the colour/callback can be cloned into every option row and the panel builder
    // (which re-runs on each open) — a `Box<dyn Fn>` can't be shared, but the widget needs to.
    let color: Rc<dyn Fn() -> Color> = Rc::from(props.color);
    let on_select: Option<Rc<dyn Fn(u32)>> =
        props.on_select.map(|f| -> Rc<dyn Fn(u32)> { Rc::from(f) });
    let open = signal(false);

    // Trigger: a bordered button with the static label; a tap toggles `open`.
    // `auto` (measured) so the label has width in this row; `single_line` only sets height → width 0 → invisible.
    let label_text = Text::auto(
        ctx,
        move || label.to_string(),
        LayoutStyle::new(),
        || TextStyle::new(14.0, INK),
    )?;
    let trigger_style = {
        let color = color.clone();
        move |_r: Rect| trigger_rect_style(color.as_ref())
    };
    let toggle = {
        let open = open.clone();
        move || open.update(|o| *o = !*o)
    };
    let trigger = StyledContainer::new(
        ctx,
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .width(PANEL_WIDTH)
            .height(TRIGGER_HEIGHT)
            .padding_horizontal(12.0),
        trigger_style,
        vec![box_item(label_text)],
    )?
    .on_press(toggle);
    let trigger_node = trigger.layout_node();
    // The trigger's laid-out rect positions the panel; `track_layout` returns the signal now (default rect)
    // and layout fills it in, so by the time the panel is built (on open) the trigger's rect is known.
    let trigger_rect =
        track_layout(&WidgetCtx::handle(), trigger_node).expect("trigger node is registered");

    // Open/close is modelled exactly like a reactive `if $open`: a single-item `ReactiveList` keyed on the
    // boolean. Building the overlay lazily (only while open) is what lets it portal correctly — the page has
    // been laid out by then, so the overlay attaches to the viewport host and the trigger's rect is known.
    let overlay_holder = ReactiveList::new(
        ctx,
        {
            let open = open.clone();
            move || vec![open.get()]
        },
        |o: &bool| *o,
        move |ctx: &mut WidgetCtx, is_open: bool| -> Result<Box<dyn LayoutItem>, LayoutError> {
            if !is_open {
                // Closed: an empty placeholder (no overlay registered, so nothing blocks the page).
                return Ok(box_item(Container::new(ctx, LayoutStyle::new(), vec![])?));
            }
            let mut rows: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(items.len());
            for (i, item_label) in items.iter().enumerate() {
                let idx = i as u32;
                let item_label = *item_label;
                let hover_style = {
                    let color = color.clone();
                    move |_r: Rect| option_row_hover_style(color.as_ref())
                };
                let on_press = {
                    let on_select = on_select.clone();
                    let open = open.clone();
                    move || {
                        if let Some(cb) = &on_select {
                            cb(idx);
                        }
                        open.set(false);
                    }
                };
                let text = Text::auto(
                    ctx,
                    move || item_label.to_string(),
                    LayoutStyle::new(),
                    || TextStyle::new(14.0, INK),
                )?;
                let row = StyledContainer::new(
                    ctx,
                    LayoutStyle::new()
                        .flex_row()
                        .align_items(AlignItems::CENTER)
                        .height(ROW_HEIGHT)
                        .padding_horizontal(10.0),
                    |_r| option_row_style(),
                    vec![box_item(text)],
                )?
                .on_hover_style(hover_style)
                .on_press(on_press);
                rows.push(box_item(row));
            }
            // Place the definite-width panel at the trigger's bottom-left via layout margins; positioning by
            // layout keeps the panel's hit rects in world space so the items dispatch correctly.
            // Window-absolute trigger position (the overlay hoists to the window; the trigger's rect signal is
            // content-local in a shell that computes the content as a separate root). Falls back if absent.
            let anchor =
                ui_core::absolute_rect(trigger_node).unwrap_or_else(|| trigger_rect.peek());
            let panel = StyledContainer::new(
                ctx,
                LayoutStyle::new()
                    .flex_column()
                    .width(PANEL_WIDTH)
                    .padding_all(PANEL_PAD)
                    .margin_left(anchor.x)
                    .margin_top(anchor.y + anchor.height),
                |_r| panel_rect_style(),
                rows,
            )?
            // Swallow clicks on the panel's own padding so they don't dismiss via the backdrop.
            .on_press(|| {});
            // A transparent full-viewport backdrop inside a BLOCKING overlay: stops hover/clicks bleeding
            // through to the page behind and dismisses the menu when a click lands outside the panel.
            let close = {
                let open = open.clone();
                move || open.set(false)
            };
            let backdrop = StyledContainer::new(
                ctx,
                LayoutStyle::new().flex_column().flex_grow(1.0),
                |_r| RectStyle::default(),
                vec![box_item(panel)],
            )?
            .on_press(close);
            let overlay = Overlay::new(
                ctx,
                LayoutStyle::new().flex_column(),
                vec![box_item(backdrop)],
            )?;
            Ok(box_item(overlay))
        },
    )?;

    // The widget owns both the trigger and the (portaling) overlay holder; the holder's placeholder takes no
    // space in the column, so only the trigger participates in flow layout.
    let root = Container::new(
        ctx,
        LayoutStyle::new().flex_column(),
        vec![box_item(trigger), box_item(overlay_holder)],
    )?;
    Ok(box_item(root))
}

/// The accent used for the trigger border and item hover: the caller's colour when set, else the theme
/// accent, else a neutral blue. Re-read every frame (inside the style closures) so a theme switch takes.
fn resolve_accent(color: &dyn Fn() -> Color) -> Color {
    let c = color();
    if c != Color::TRANSPARENT {
        return c;
    }
    use_widget_theme()
        .map(|t| t.widget_primary())
        .unwrap_or(Color::rgba(0.24, 0.47, 0.98, 1.0))
}

fn trigger_rect_style(color: &dyn Fn() -> Color) -> RectStyle {
    RectStyle::default()
        .with_fill(SURFACE)
        .with_stroke(Stroke::new(resolve_accent(color), 1.0))
        .with_radius(BorderRadius::all(6.0))
}

fn panel_rect_style() -> RectStyle {
    RectStyle::default()
        .with_fill(SURFACE)
        .with_stroke(Stroke::new(BORDER, 1.0))
        .with_radius(BorderRadius::all(8.0))
}

fn option_row_style() -> RectStyle {
    RectStyle::default().with_radius(BorderRadius::all(4.0))
}

fn option_row_hover_style(color: &dyn Fn() -> Color) -> RectStyle {
    RectStyle::default()
        .with_fill(resolve_accent(color).with_alpha(0.10))
        .with_radius(BorderRadius::all(4.0))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerButton, PointerSource};
    use ui_core::{
        ComponentList, EventResult, WidgetCtx, compute_layout, dispatch_overlays, relayout_if_dirty,
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

    // Mirror the runner: consult the overlay registry first, then walk the tree only if no overlay
    // consumed the event (the anchored panel routes through the registry, the trigger through the tree).
    fn route(tree: &mut ComponentList, event: &Event) {
        if dispatch_overlays(event) == EventResult::Ignored {
            tree.on_event(event);
        }
    }

    // Construction: a menu builds headless, lays out (the trigger takes its fixed size), and renders.
    #[test]
    fn builds_and_lays_out() {
        let mut ctx = WidgetCtx::new();
        let item = menu(
            &mut ctx,
            MenuProps {
                label: "Actions",
                items: vec!["Rename", "Duplicate", "Delete"],
                ..Default::default()
            },
        )
        .unwrap();
        let root_node = item.layout_node();
        let root_rect = track_layout(&ctx, root_node).unwrap();
        compute_layout(
            &mut ctx,
            root_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        assert!(
            root_rect.get().height >= TRIGGER_HEIGHT - 0.5,
            "closed menu is at least the trigger tall: {:?}",
            root_rect.get()
        );
        let tree = ComponentList::new(item);
        let _ = tree.commands();
    }

    // Picking an item fires on_select with its index and closes the menu.
    #[test]
    fn selecting_an_item_fires_on_select_and_closes() {
        let mut ctx = WidgetCtx::new();
        let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let item = menu(
            &mut ctx,
            MenuProps {
                label: "Actions",
                items: vec!["Rename", "Duplicate", "Delete"],
                on_select: Some(Box::new(move |i| sink.set(Some(i)))),
                ..Default::default()
            },
        )
        .unwrap();
        // The widget's own root is the parent-less layout host, laid out at the origin: the trigger sits at
        // (0,0) and the panel anchors directly below it, so click points are computable from the constants.
        let root_node = item.layout_node();
        compute_layout(
            &mut ctx,
            root_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(item);
        let _ = tree.commands();

        // A tap on the trigger (through the tree) opens the panel.
        let tx = (PANEL_WIDTH / 2.0) as f64;
        let ty = (TRIGGER_HEIGHT / 2.0) as f64;
        route(&mut tree, &press(tx, ty));
        route(&mut tree, &release(tx, ty));
        relayout_if_dirty();

        // Item index 1 ("Duplicate") sits at the trigger's bottom + panel padding + one row.
        let ox = (PANEL_WIDTH / 2.0) as f64;
        let oy = (TRIGGER_HEIGHT + PANEL_PAD + ROW_HEIGHT + ROW_HEIGHT / 2.0) as f64;
        route(&mut tree, &press(ox, oy));
        route(&mut tree, &release(ox, oy));

        assert_eq!(
            seen.get(),
            Some(1),
            "picking the second item fires on_select(1)"
        );
        assert_eq!(
            dispatch_overlays(&press(ox, oy)),
            EventResult::Ignored,
            "the menu closes after a pick"
        );
    }
}
