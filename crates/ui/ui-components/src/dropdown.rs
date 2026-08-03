//! Shared dropdown scaffold (constants, style fns, trigger + overlay/backdrop/anchoring) behind `menu` and `select`.

use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{AlignItems, LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke, TextStyle};
use theme_core::use_theme_tokens;
use ui_core::{
    Container, LayoutItem, Overlay, ReactiveList, StyledContainer, Text, box_item, track_layout,
};

use crate::shared;

/// The trigger's fixed size and the dropdown panel's geometry. Kept as constants (not content-sized) so the
/// panel lines up under the trigger and so the widgets' hit geometry is deterministic (their tests compute
/// click points from these).
pub(crate) const PANEL_WIDTH: f32 = 180.0;
pub(crate) const TRIGGER_HEIGHT: f32 = 36.0;
pub(crate) const ROW_HEIGHT: f32 = 32.0;
pub(crate) fn panel_pad() -> f32 {
    shared::spacing() * 0.5
}

/// The trigger + anchored-panel scaffold shared by `menu` and `select`: a bordered trigger button opens a
/// blocking overlay whose transparent backdrop dismisses on click-away, and whose anchored panel lists `rows`;
/// picking a row optionally writes into `selected`, fires `on_pick`, and closes.
///
/// `trigger_label` is a closure so `select` can track its bound signal reactively while `menu` supplies a
/// static label. `selected` drives the bound-selection behaviour: `Some` writes the picked index back and
/// highlights the selected row; `None` (menu) skips both, leaving one-shot actions.
pub(crate) fn dropdown(
    trigger_label: impl Fn() -> String + 'static,
    rows: Vec<&'static str>,
    color: Box<dyn Fn() -> Color>,
    on_pick: Option<Box<dyn Fn(u32)>>,
    selected: Option<RwSignal<u32>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // Erased to `Rc` so the colour/callback can be cloned into the trigger, every option row, and the panel
    // builder (which re-runs on each open) — a `Box<dyn Fn>` can't be shared, but the widget needs to.
    let color: shared::ReactiveColor = Rc::from(color);
    let on_pick: Option<Rc<dyn Fn(u32)>> = on_pick.map(|f| -> Rc<dyn Fn(u32)> { Rc::from(f) });
    let open = signal(false);

    // Trigger: a bordered button with the label; a tap toggles `open`.
    // `auto` (measured) so the label has width in this row; `single_line` only sets height → width 0 → invisible.
    let label_text = Text::auto(trigger_label, LayoutStyle::new(), || {
        TextStyle::new(shared::font_size(), shared::ink())
    })?;
    let trigger_style = {
        let color = color.clone();
        move |_r: Rect| trigger_rect_style(color.as_ref())
    };
    let toggle = {
        let open = open.clone();
        move || open.update(|o| *o = !*o)
    };
    let trigger = StyledContainer::new(
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
    let trigger_rect = track_layout(trigger_node).expect("trigger node is registered");

    // Open/close is modelled exactly like a reactive `if $open`: a single-item `ReactiveList` keyed on the
    // boolean. Building the overlay lazily (only while open) is what lets it portal correctly — the page has
    // been laid out by then, so the overlay attaches to the viewport host and the trigger's rect is known.
    let overlay_holder = ReactiveList::new(
        {
            let open = open.clone();
            move || vec![open.get()]
        },
        |o: &bool| *o,
        move |is_open: bool| -> Result<Box<dyn LayoutItem>, LayoutError> {
            if !is_open {
                // Closed: an empty placeholder (no overlay registered, so nothing blocks the page).
                return Ok(box_item(Container::new(LayoutStyle::new(), vec![])?));
            }
            let mut row_items: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(rows.len());
            for (i, label) in rows.iter().enumerate() {
                let idx = i as u32;
                let label = *label;
                let row_style = {
                    let selected = selected.clone();
                    let color = color.clone();
                    // Read `selected` at view time (not here in the builder) so re-highlighting a new
                    // selection re-renders the row instead of subscribing the list effect and rebuilding.
                    move |_r: Rect| {
                        let is_selected = selected.as_ref().is_some_and(|s| s.get() == idx);
                        option_row_style(is_selected, color.as_ref())
                    }
                };
                let hover_style = {
                    let color = color.clone();
                    move |_r: Rect| option_row_hover_style(color.as_ref())
                };
                let on_press = {
                    let selected = selected.clone();
                    let on_pick = on_pick.clone();
                    let open = open.clone();
                    move || {
                        if let Some(sel) = &selected {
                            sel.set(idx);
                        }
                        if let Some(cb) = &on_pick {
                            cb(idx);
                        }
                        open.set(false);
                    }
                };
                let text = Text::auto(
                    move || label.to_string(),
                    LayoutStyle::new(),
                    || TextStyle::new(shared::font_size(), shared::ink()),
                )?;
                let row = StyledContainer::new(
                    LayoutStyle::new()
                        .flex_row()
                        .align_items(AlignItems::CENTER)
                        .height(ROW_HEIGHT)
                        .padding_horizontal(10.0),
                    row_style,
                    vec![box_item(text)],
                )?
                .on_hover_style(hover_style)
                .on_press(on_press);
                row_items.push(box_item(row));
            }
            // Place the definite-width panel at the trigger's bottom-left via layout margins; positioning by
            // layout keeps the panel's hit rects in world space so the rows dispatch correctly. The trigger's
            // rect is window-absolute (the overlay hoists to the window), falling back to the local rect.
            let anchor = ui_core::overlay::anchor_rect(trigger_node, &trigger_rect);
            // The anchor is the trigger's rect as it stands when the panel opens: a re-resolve on a theme
            // switch keeps the margins it was built from, which is where the panel belongs either way.
            let sheet = move || {
                LayoutStyle::new()
                    .flex_column()
                    .width(PANEL_WIDTH)
                    .padding_all(panel_pad())
                    .margin_left(anchor.x)
                    .margin_top(anchor.y + anchor.height)
            };
            let panel = StyledContainer::new(sheet(), |_r| panel_rect_style(), row_items)?
                .styled_by(sheet)
                // Swallow clicks on the panel's own padding so they don't dismiss via the backdrop.
                .on_press(|| {});
            // A transparent full-viewport backdrop inside a BLOCKING overlay: stops hover/clicks bleeding
            // through to the page behind and dismisses the dropdown when a click lands outside the panel.
            let close = {
                let open = open.clone();
                move || open.set(false)
            };
            let backdrop = StyledContainer::new(
                LayoutStyle::new().flex_column().flex_grow(1.0),
                |_r| RectStyle::default(),
                vec![box_item(panel)],
            )?
            .on_press(close);
            let overlay = Overlay::new(LayoutStyle::new().flex_column(), vec![box_item(backdrop)])?;
            Ok(box_item(overlay))
        },
    )?;

    // The widget owns both the trigger and the (portaling) overlay holder; the holder's placeholder takes no
    // space in the column, so only the trigger participates in flow layout.
    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(trigger), box_item(overlay_holder)],
    )?;
    Ok(box_item(root))
}

fn trigger_rect_style(color: &dyn Fn() -> Color) -> RectStyle {
    let accent = shared::resolve(color, || {
        use_theme_tokens()
            .map(|t| t.primary())
            .unwrap_or(shared::DEFAULT_ACCENT)
    });
    RectStyle::default()
        .with_fill(shared::surface())
        .with_stroke(Stroke::new(accent, 1.0))
        .with_radius(BorderRadius::all(6.0))
}

fn panel_rect_style() -> RectStyle {
    RectStyle::default()
        .with_fill(shared::surface())
        .with_stroke(Stroke::new(shared::border(), 1.0))
        .with_radius(BorderRadius::all(8.0))
}

// A menu row (`selected: None` → always `is_selected == false`) yields the plain radius-only style; a select
// row highlights the bound choice with a faint accent fill.
fn option_row_style(is_selected: bool, color: &dyn Fn() -> Color) -> RectStyle {
    let radius = BorderRadius::all(4.0);
    if is_selected {
        let accent = shared::resolve(color, || {
            use_theme_tokens()
                .map(|t| t.primary())
                .unwrap_or(shared::DEFAULT_ACCENT)
        });
        RectStyle::default()
            .with_fill(accent.with_alpha(0.14))
            .with_radius(radius)
    } else {
        RectStyle::default().with_radius(radius)
    }
}

fn option_row_hover_style(color: &dyn Fn() -> Color) -> RectStyle {
    let accent = shared::resolve(color, || {
        use_theme_tokens()
            .map(|t| t.primary())
            .unwrap_or(shared::DEFAULT_ACCENT)
    });
    RectStyle::default()
        .with_fill(accent.with_alpha(0.10))
        .with_radius(BorderRadius::all(4.0))
}
